// Copyright © 2016, Canal TP and/or its affiliates. All rights reserved.
//
// This file is part of Navitia,
//     the software to build cool stuff with public transport.
//
// Hope you'll enjoy and contribute to this project,
//     powered by Canal TP (www.canaltp.fr).
// Help us simplify mobility and open public transport:
//     a non ending quest to the responsive locomotion way of traveling!
//
// LICENCE: This program is free software; you can redistribute it
// and/or modify it under the terms of the GNU Affero General Public
// License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see
// <http://www.gnu.org/licenses/>.
//
// Stay tuned using
// twitter @navitia
// IRC #navitia on freenode
// https://groups.google.com/d/forum/navitia
// www.navitia.io

use super::osm_store::{Getter, Kind, ObjWrapper};
use super::osm_utils::get_way_coord;
use super::OsmPbfReader;
use crate::admin_geofinder::AdminGeoFinder;
use crate::{labels, utils, Error};
use failure::ResultExt;
use osmpbfreader::StoreObjs;
use slog_scope::{debug, info};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

// This structure is used to prevent having multiple streets with the same name
// in the same admin city level.
#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct StreetKey {
    pub name: String,
    pub admins: BTreeSet<Arc<mimir::Admin>>,
}

// This function generates the list of streets found by reading from the OSM PBF Reader.
// It does a lot of work trying to merge parts of streets that may be otherwise split in OSM to avoid
// returning duplicate answers to the user.
//
// Sometimes, streets can be divided into several "way"s that still have the same street name.
// The reason why a street is divided may be that a part of the street become
// a bridge/tunnel/etc. In this case, a "relation" tagged with (type = associatedStreet) is used
// to group all these "way"s. In order not to have duplicates in autocompletion, we should tag
// the osm ways in the relation not to index them twice.
// The parameter db_file will be used if we use an in memory map to store the streets while we
// work on them, or a sqlite database (on file).
pub fn streets(
    pbf: &mut OsmPbfReader,
    admins_geofinder: &AdminGeoFinder,
    db_file: &Option<PathBuf>,
    db_buffer_size: usize,
) -> Result<Vec<mimir::Street>, Error> {
    // First we read from the pbf to extract all the valid ways, which can be either
    // - ways
    // - relations with the tag 'associatedStreet'

    // This is the list of highway that we don't want to index
    // See [OSM Key Highway](https://wiki.openstreetmap.org/wiki/Key:highway) for background.
    const INVALID_HIGHWAY: &'static [&'static str] =
        &["bus_guideway", "escape", "bus_stop", "elevator", "platform"];

    // For the object to be a valid street, it needs to be an osm highway of a valid type,
    // or a relation of type associatedStreet.
    fn is_valid_way(obj: &osmpbfreader::OsmObj) -> bool {
        match *obj {
            osmpbfreader::OsmObj::Way(ref way) => {
                way.tags.get("highway").map_or(false, |v| {
                    !v.is_empty() && !INVALID_HIGHWAY.iter().any(|&k| k == v)
                }) && way.tags.get("name").map_or(false, |v| !v.is_empty())
            }
            osmpbfreader::OsmObj::Relation(ref rel) => rel
                .tags
                .get("type")
                .map_or(false, |v| v == "associatedStreet"),
            _ => false,
        }
    }
    debug!("reading pbf...");
    let mut objs_map = ObjWrapper::new(db_file, db_buffer_size)?;
    pbf.get_objs_and_deps_store(is_valid_way, &mut objs_map)
        .context("Error occurred when reading pbf")?;
    debug!("reading pbf done.");

    // Second we build a list of all the streets that are part of a relation.
    // TODO street_rel
    let mut street_rel: BTreeSet<osmpbfreader::OsmId> = BTreeSet::new();
    let mut street_list: Vec<mimir::Street> = vec![];

    objs_map.for_each_filter(Kind::Relation, |obj| {
        let rel = obj.relation().expect("impossible unwrap failure occured");
        let way_name = rel.tags.get("name");
        rel.refs
            .iter()
            .filter(|ref_obj| ref_obj.member.is_way() && ref_obj.role == "street")
            .filter_map(|ref_obj| {
                let obj = objs_map.get(&ref_obj.member)?;
                let way = obj.way()?;
                let way_name = way_name.or_else(|| way.tags.get("name"))?;
                let admins = get_street_admin(admins_geofinder, &objs_map, way);
                let country_codes = utils::find_country_codes(admins.iter().map(|a| a.deref()));
                let street_label = labels::format_street_label(
                    &way_name,
                    admins.iter().map(|a| a.deref()),
                    &country_codes,
                );
                let coord = get_way_coord(&objs_map, way);
                Some(mimir::Street {
                    id: format!("street:osm:relation:{}", rel.id.0.to_string()),
                    name: way_name.to_string(),
                    label: street_label,
                    weight: 0.,
                    zip_codes: utils::get_zip_codes_from_admins(&admins),
                    administrative_regions: admins,
                    coord: get_way_coord(&objs_map, way),
                    approx_coord: Some(coord.into()),
                    distance: None,
                    country_codes,
                    context: None,
                })
            })
            .next()
            .map(|street| street_list.push(street));

        // Add osmid of all the relation members in the set
        // We don't create any street for all the osmid present in street_rel
        for ref_obj in &rel.refs {
            if ref_obj.member.is_way() {
                street_rel.insert(ref_obj.member);
            }
        }
    });

    // we merge all the ways with a key = way_name + admin list of level(=city_level)
    // we use a map NameAdminMap <key, value> to manage the merging of ways
    let mut name_admin_map: BTreeMap<StreetKey, Vec<osmpbfreader::OsmId>> = BTreeMap::default();
    objs_map.for_each_filter(Kind::Way, |obj| {
        let osmid = obj.id();
        if street_rel.contains(&osmid) {
            return;
        }
        if let Some(way) = obj.way() {
            if let Some(name) = way.tags.get("name") {
                let name = name.to_string();
                let admins = get_street_admin(admins_geofinder, &objs_map, way)
                    .into_iter()
                    .filter(|admin| admin.is_city())
                    .collect();
                name_admin_map
                    .entry(StreetKey { name, admins })
                    .or_insert(vec![])
                    .push(osmid);
            }
        }
    });

    // Create a street for each way with osmid present in objs_map
    let streets = name_admin_map.values().filter_map(|way_ids| {
        let min_id = way_ids.iter().min()?;
        let obj = objs_map.get(&min_id)?;
        let way = obj.way()?;
        let name = way.tags.get("name")?.to_string();
        let admins = get_street_admin(admins_geofinder, &objs_map, way);

        let country_codes = utils::find_country_codes(admins.iter().map(|a| a.deref()));
        let street_label =
            labels::format_street_label(&name, admins.iter().map(|a| a.deref()), &country_codes);
        let coord = get_way_coord(&objs_map, way);
        Some(mimir::Street {
            id: format!("street:osm:way:{}", way.id.0.to_string()),
            label: street_label,
            name,
            weight: 0.,
            zip_codes: utils::get_zip_codes_from_admins(&admins),
            administrative_regions: admins,
            coord: get_way_coord(&objs_map, way),
            approx_coord: Some(coord.into()),
            distance: None,
            country_codes,
            context: None,
        })
    });
    street_list.extend(streets);

    Ok(street_list)
}

fn get_street_admin<T: StoreObjs + Getter>(
    admins_geofinder: &AdminGeoFinder,
    obj_map: &T,
    way: &osmpbfreader::objects::Way,
) -> Vec<Arc<mimir::Admin>> {
    /*
        To avoid corner cases where the ends of the way are near
        administrative boundaries, the geofinder is called
        on a middle node.
    */
    let nb_nodes = way.nodes.len();
    way.nodes
        .iter()
        .skip(nb_nodes / 2)
        .filter_map(|node_id| obj_map.get(&(*node_id).into()))
        .filter_map(|node_obj| {
            node_obj.node().map(|node| geo_types::Coordinate {
                x: node.lon(),
                y: node.lat(),
            })
        })
        .next()
        .map_or(vec![], |c| admins_geofinder.get(&c))
}

pub fn compute_street_weight(streets: &mut Vec<mimir::Street>) {
    for st in streets {
        for admin in &mut st.administrative_regions {
            if admin.is_city() {
                st.weight = admin.weight;
                break;
            }
        }
    }
}
