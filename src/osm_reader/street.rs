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

extern crate geo;
extern crate mimir;
extern crate osmpbfreader;

use admin_geofinder::AdminGeoFinder;
use std::collections::{BTreeSet, BTreeMap};
use std::rc::Rc;
use utils::{format_label, get_zip_codes_from_admins};
use super::osm_utils::get_way_coord;
use super::OsmPbfReader;

pub type AdminSet = BTreeSet<Rc<mimir::Admin>>;
pub type NameAdminMap = BTreeMap<StreetKey, Vec<osmpbfreader::OsmId>>;
pub type StreetsVec = Vec<mimir::Street>;
pub type StreetWithRelationSet = BTreeSet<osmpbfreader::OsmId>;

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct StreetKey {
    pub name: String,
    pub admins: AdminSet,
}

pub fn streets(pbf: &mut OsmPbfReader,
               admins_geofinder: &AdminGeoFinder,
               city_level: u32)
               -> StreetsVec {

    fn is_valid_obj(obj: &osmpbfreader::OsmObj) -> bool {
        match *obj {
            osmpbfreader::OsmObj::Way(ref way) => {
                way.tags.get("highway").map_or(false, |v| !v.is_empty()) &&
                way.tags.get("name").map_or(false, |v| !v.is_empty())
            }
            osmpbfreader::OsmObj::Relation(ref rel) => {
                rel.tags.get("type").map_or(false, |v| v == "associatedStreet")
            }
            _ => false,
        }
    }
    info!("reading pbf...");
    let objs_map = pbf.get_objs_and_deps(is_valid_obj).unwrap();
    info!("reading pbf done.");
    let mut street_rel: StreetWithRelationSet = BTreeSet::new();
    let mut street_list: StreetsVec = vec![];
    // Sometimes, streets can be divided into several "way"s that still have the same street name.
    // The reason why a street is divided may be that a part of the street become
    // a bridge/tunnel/etc. In this case, a "relation" tagged with (type = associatedStreet) is used
    // to group all these "way"s. In order not to have duplicates in autocompletion, we should tag
    // the osm ways in the relation not to index them twice.

    for rel in objs_map.iter().filter_map(|(_, obj)| obj.relation()) {
        let way_name = rel.tags.get("name");
        for ref_obj in &rel.refs {

            use mdo::option::*;
            let objs_map = &objs_map;
            let street_list = &mut street_list;
            let admins_geofinder = &admins_geofinder;

            let inserted = mdo! {
                when ref_obj.member.is_way();
                when ref_obj.role == "street";
                obj =<< objs_map.get(&ref_obj.member);
                way =<< obj.way();
                way_name =<< way_name.or_else(|| way.tags.get("name"));
                let admin = get_street_admin(admins_geofinder, objs_map, way);
                ret ret(street_list.push(mimir::Street {
                    id: way.id.0.to_string(),
                    street_name: way_name.to_string(),
                    label: format_label(&admin, city_level, way_name),
                    weight: 0.,
                    zip_codes: get_zip_codes_from_admins(&admin),
                    administrative_regions: admin,
                    coord: get_way_coord(objs_map, way),
                }))
            };
            if inserted.is_some() {
                break;
            }
        }

        // Add osmid of all the relation members in de set
        // We don't create any street for all the osmid present in street_rel
        for ref_obj in &rel.refs {
            if ref_obj.member.is_way() {
                street_rel.insert(ref_obj.member);
            }
        }
    }

    // we merge all the ways with a key = way_name + admin list of level(=city_level)
    // we use a map NameAdminMap <key, value> to manage the merging of ways
    let mut name_admin_map: NameAdminMap = BTreeMap::new();
    for (osmid, obj) in &objs_map {
        if street_rel.contains(osmid) {
            continue;
        }
        use mdo::option::*;
        let admins_geofinder = &admins_geofinder;
        let objs_map = &objs_map;
        let name_admin_map = &mut name_admin_map;
        mdo! {
            way =<< obj.way();
            let admins: BTreeSet<Rc<mimir::Admin>> = get_street_admin(admins_geofinder,
                objs_map, way)
            	.into_iter()
            	.filter(|admin| admin.level == city_level)
            	.collect();

            way_name =<< way.tags.get("name");
            let key = StreetKey{name: way_name.to_string(), admins: admins};
            ret ret(name_admin_map.entry(key).or_insert(vec![]).push(*osmid))
        };
    }

    // Create a street for each way with osmid present in in objs_map
    for way_ids in name_admin_map.values() {
        use mdo::option::*;
        let objs_map = &objs_map;
        let street_list = &mut street_list;
        let admins_geofinder = &admins_geofinder;
        mdo! {
            obj =<< objs_map.get(&way_ids[0]);
            way =<< obj.way();
            way_name =<< way.tags.get("name");
            let admins = get_street_admin(admins_geofinder, objs_map, way);
            ret ret(street_list.push(mimir::Street {
                id: way.id.0.to_string(),
                street_name: way_name.to_string(),
                label: format_label(&admins, city_level, way_name),
                weight: 0.,
                zip_codes: get_zip_codes_from_admins(&admins),
                administrative_regions: admins,
                coord: get_way_coord(objs_map, way),
            }))
        };
    }

    street_list
}

fn get_street_admin(admins_geofinder: &AdminGeoFinder,
                    obj_map: &BTreeMap<osmpbfreader::OsmId, osmpbfreader::OsmObj>,
                    way: &osmpbfreader::objects::Way)
                    -> Vec<Rc<mimir::Admin>> {
    // for the moment we consider that the coord of the way is the coord of it's first node
    way.nodes
        .iter()
        .filter_map(|node_id| obj_map.get(&(*node_id).into()))
        .filter_map(|node_obj| node_obj.node())
        .map(|node| {
            geo::Coordinate {
                x: node.lat(),
                y: node.lon(),
            }
        })
        .next()
        .map_or(vec![], |c| admins_geofinder.get(&c))
}

pub fn compute_street_weight(streets: &mut StreetsVec, city_level: u32) {
    for st in streets {
        for admin in &mut st.administrative_regions {
            if admin.level == city_level {
                st.weight = admin.weight.get();
                break;
            }
        }
    }
}
