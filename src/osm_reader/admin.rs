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

extern crate osmpbfreader;
extern crate mimir;

use admin_geofinder::AdminGeoFinder;
use boundaries::{build_boundary, make_centroid};
use std::cell::Cell;
use std::collections::BTreeSet;
use itertools::Itertools;
use super::OsmPbfReader;
pub type StreetsVec = Vec<mimir::Street>;

#[derive(Debug)]
pub struct AdminMatcher {
    admin_levels: BTreeSet<u32>,
}

impl AdminMatcher {
    pub fn new(levels: BTreeSet<u32>) -> AdminMatcher {
        AdminMatcher { admin_levels: levels }
    }

    pub fn is_admin(&self, obj: &osmpbfreader::OsmObj) -> bool {
        match *obj {
            osmpbfreader::OsmObj::Relation(ref rel) => {
                rel.tags.get("boundary").map_or(false, |v| v == "administrative") &&
                rel.tags.get("admin_level").map_or(false, |lvl| {
                    self.admin_levels.contains(&lvl.parse::<u32>().unwrap_or(0))
                })
            }
            _ => false,
        }
    }
}

pub fn administrative_regions(pbf: &mut OsmPbfReader, levels: BTreeSet<u32>) -> Vec<mimir::Admin> {
    let mut administrative_regions = Vec::<mimir::Admin>::new();
    let mut insee_inserted = BTreeSet::default();
    let matcher = AdminMatcher::new(levels);
    info!("reading pbf...");
    let objects = pbf.get_objs_and_deps(|o| matcher.is_admin(o)).unwrap();
    info!("reading pbf done.");
    // load administratives regions
    for obj in objects.values() {
        if !matcher.is_admin(obj) {
            continue;
        }
        if let osmpbfreader::OsmObj::Relation(ref relation) = *obj {
            let level = relation.tags.get("admin_level").and_then(|s| s.parse().ok());
            let level = match level {
                None => {
                    warn!("relation/{} ({}): invalid admin_level: {:?}, skipped",
                          relation.id.0,
                          relation.tags.get("name").map_or("", String::as_str),
                          relation.tags.get("admin_level"));
                    continue;
                }
                Some(l) => l,
            };
            // administrative region with name ?
            let name = match relation.tags.get("name") {
                Some(val) => val,
                None => {
                    warn!("relation/{}: adminstrative region without name, skipped",
                          relation.id.0);
                    continue;
                }
            };

            // admininstrative region without coordinates
            let coord_center = relation.refs
                .iter()
                .find(|r| r.role == "admin_centre")
                .and_then(|r| objects.get(&r.member))
                .and_then(|o| o.node())
                .map(|node| mimir::Coord::new(node.lat(), node.lon()));
            let (admin_id, insee_id) = match relation.tags
                .get("ref:INSEE")
                .map(|v| v.trim_left_matches('0')) {
                Some(val) if !insee_inserted.contains(val) => {
                    insee_inserted.insert(val.to_string());
                    (format!("admin:fr:{}", val), val)
                }
                Some(val) => {
                    let id = format!("admin:osm:{}", relation.id.0);
                    warn!("relation/{}: have the INSEE {} that is already used, using {} as id",
                          relation.id.0,
                          val,
                          id);
                    (id, val)
                }
                None => (format!("admin:osm:{}", relation.id.0), ""),
            };

            let zip_code = relation.tags
                .get("addr:postcode")
                .or_else(|| relation.tags.get("postal_code"))
                .map_or("", |val| &val[..]);
            let zip_codes = zip_code.split(';')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .sorted();
            let boundary = build_boundary(relation, &objects);
            let admin = mimir::Admin {
                id: admin_id,
                insee: insee_id.to_string(),
                level: level,
                name: name.to_string(),
                label: format!("{}{}", name.to_string(), format_zip_codes(&zip_codes)),
                zip_codes: zip_codes,
                weight: Cell::new(0.),
                coord: coord_center.unwrap_or_else(|| make_centroid(&boundary)),
                boundary: boundary,
            };
            administrative_regions.push(admin);
        }
    }
    administrative_regions
}

pub fn compute_admin_weight(streets: &StreetsVec, admins_geofinder: &AdminGeoFinder) {
    let mut max = 1.;
    for st in streets {
        for admin in &st.administrative_regions {
            admin.weight.set(admin.weight.get() + 1.);
            max = f64::max(max, admin.weight.get());
        }
    }

    for admin in admins_geofinder.admins_without_boundary() {
        admin.weight.set(admin.weight.get() / max);
    }
}

fn format_zip_codes(zip_codes: &[String]) -> String {
    match zip_codes.len() {
        0 => "".to_string(),
        1 => format!(" ({})", zip_codes.first().unwrap()),
        _ => {
            format!(" ({}-{})",
                    zip_codes.first().unwrap(),
                    zip_codes.last().unwrap())
        }
    }
}
