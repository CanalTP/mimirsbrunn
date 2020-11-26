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

use super::osm_utils::get_way_coord;
use super::osm_utils::make_centroid;
use super::OsmPbfReader;
use crate::admin_geofinder::AdminGeoFinder;
use crate::{labels, settings::osm2mimir::Settings, utils};
use mimir::{rubber, Poi, PoiType};
use osm_boundaries_utils::build_boundary;
use serde::{Deserialize, Serialize};
use slog_scope::{info, warn};
use std::collections::BTreeMap;
use std::error::Error;
use std::io;
use std::ops::Deref;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OsmTagsFilter {
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Rule {
    pub osm_tags_filters: Vec<OsmTagsFilter>,
    #[serde(rename = "type")]
    pub poi_type_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PoiConfig {
    #[serde(rename = "types")]
    pub poi_types: Vec<PoiType>,
    pub rules: Vec<Rule>,
}

impl Default for PoiConfig {
    fn default() -> Self {
        let default_settings: Settings = toml::from_str(include_str!("../../config/osm2mimir-default.toml"))
            .expect("Could not read default osm2mimir settings for default poi types from osm2mimir-default.toml");
        let config = default_settings
            .poi
            .and_then(|poi| poi.config)
            .expect("osm2mimir-default.toml does not contain default poi types");
        config.check().unwrap();
        config
    }
}
impl PoiConfig {
    pub fn from_reader<R: io::Read>(r: R) -> Result<PoiConfig, Box<dyn Error>> {
        let config: PoiConfig = serde_json::from_reader(r)?;
        config.check()?;
        Ok(config)
    }
    pub fn is_poi(&self, tags: &osmpbfreader::Tags) -> bool {
        self.get_poi_type(tags).is_some()
    }
    pub fn get_poi_id(&self, tags: &osmpbfreader::Tags) -> Option<&str> {
        self.get_poi_type(tags).map(|poi_type| poi_type.id.as_str())
    }
    pub fn get_poi_type(&self, tags: &osmpbfreader::Tags) -> Option<&PoiType> {
        self.rules
            .iter()
            .find(|rule| {
                rule.osm_tags_filters
                    .iter()
                    .all(|f| tags.get(&f.key).map_or(false, |v| v == &f.value))
            })
            .and_then(|rule| {
                self.poi_types
                    .iter()
                    .find(|poi_type| poi_type.id == rule.poi_type_id)
            })
    }
    pub fn check(&self) -> Result<(), Box<dyn Error>> {
        use std::collections::BTreeSet;
        let mut ids = BTreeSet::<&str>::new();
        for poi_type in &self.poi_types {
            if !ids.insert(&poi_type.id) {
                return Err(format!("poi_type_id {:?} present several times", poi_type.id).into());
            }
        }
        for rule in &self.rules {
            if !ids.contains(rule.poi_type_id.as_str()) {
                return Err(
                    format!("poi_type_id {:?} in a rule not declared", rule.poi_type_id).into(),
                );
            }
        }
        Ok(())
    }
}

fn make_properties(tags: &osmpbfreader::Tags) -> Vec<mimir::Property> {
    tags.iter()
        .map(|property| mimir::Property {
            key: property.0.to_string(),
            value: property.1.to_string(),
        })
        .collect()
}

fn parse_poi(
    osmobj: &osmpbfreader::OsmObj,
    obj_map: &BTreeMap<osmpbfreader::OsmId, osmpbfreader::OsmObj>,
    matcher: &PoiConfig,
    admins_geofinder: &AdminGeoFinder,
) -> Option<mimir::Poi> {
    let poi_type = match matcher.get_poi_type(osmobj.tags()) {
        Some(poi_type) => poi_type,
        None => {
            warn!(
                "The poi {:?} has no tags even if it passes the filters",
                osmobj.id()
            );
            return None;
        }
    };
    let (id, coord) = match *osmobj {
        osmpbfreader::OsmObj::Node(ref node) => (
            format_poi_id("node", node.id.0),
            mimir::Coord::new(node.lon(), node.lat()),
        ),
        osmpbfreader::OsmObj::Way(ref way) => {
            (format_poi_id("way", way.id.0), get_way_coord(obj_map, way))
        }
        osmpbfreader::OsmObj::Relation(ref relation) => (
            format_poi_id("relation", relation.id.0),
            make_centroid(&build_boundary(relation, obj_map)),
        ),
    };

    let name = osmobj.tags().get("name").unwrap_or(&poi_type.name);

    if coord.is_default() {
        info!(
            "The poi {} is rejected, cause: could not compute coordinates.",
            id
        );
        return None;
    }

    let adms = admins_geofinder.get(&coord);
    let zip_codes = match osmobj.tags().get("addr:postcode") {
        Some(val) if !val.is_empty() => vec![val.clone()],
        _ => utils::get_zip_codes_from_admins(&adms),
    };
    let country_codes = utils::find_country_codes(adms.iter().map(|a| a.deref()));
    Some(mimir::Poi {
        id,
        name: name.to_string(),
        label: labels::format_poi_label(name, adms.iter().map(|a| a.deref()), &country_codes),
        coord,
        approx_coord: Some(coord.into()),
        zip_codes,
        administrative_regions: adms,
        weight: 0.,
        poi_type: poi_type.clone(),
        properties: make_properties(osmobj.tags()),
        address: None,
        names: mimir::I18nProperties::default(),
        labels: mimir::I18nProperties::default(),
        distance: None,
        country_codes,
        context: None,
    })
}

fn format_poi_id(osm_type: &str, id: i64) -> String {
    format!("poi:osm:{}:{}", osm_type, id)
}

pub fn pois(
    pbf: &mut OsmPbfReader,
    matcher: &PoiConfig,
    admins_geofinder: &AdminGeoFinder,
) -> Vec<Poi> {
    let objects = pbf.get_objs_and_deps(|o| matcher.is_poi(o.tags())).unwrap();
    objects
        .iter()
        .filter(|&(_, obj)| matcher.is_poi(obj.tags()))
        .filter_map(|(_, obj)| parse_poi(obj, &objects, matcher, admins_geofinder))
        .collect()
}

pub fn compute_poi_weight(pois_vec: &mut [Poi]) {
    for poi in pois_vec {
        for admin in &mut poi.administrative_regions {
            if admin.is_city() {
                poi.weight = admin.weight;
                break;
            }
        }
    }
}

pub fn add_address(pois_vec: &mut [Poi], rubber: &mut rubber::Rubber) {
    for poi in pois_vec {
        poi.address = rubber
            .get_address(&poi.coord)
            .ok()
            .and_then(|addrs| addrs.into_iter().next())
            .map(|addr| addr.address().unwrap());
        if poi.address.is_none() {
            warn!("The poi {:?} {:?} doesn't have address", poi.id, poi.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::io;
    fn tags(v: &[(&str, &str)]) -> osmpbfreader::Tags {
        v.iter().map(|&(k, v)| (k.into(), v.into())).collect()
    }
    fn from_str(s: &str) -> Result<PoiConfig, Box<dyn Error>> {
        PoiConfig::from_reader(io::Cursor::new(s))
    }
    #[test]
    fn default_test() {
        let c = PoiConfig::default();
        assert!(c.get_poi_id(&tags(&[])).is_none());
        for s in &[
            "college",
            "university",
            "theatre",
            "hospital",
            "post_office",
            "bicycle_rental",
            "bicycle_parking",
            "parking",
            "police",
        ] {
            assert_eq!(
                format!("poi_type:amenity:{}", s),
                c.get_poi_id(&tags(&[("amenity", s)])).unwrap()
            );
        }
        for s in &["garden", "park"] {
            assert_eq!(
                format!("poi_type:leisure:{}", s),
                c.get_poi_id(&tags(&[("leisure", s)])).unwrap()
            );
        }
    }
    #[test]
    fn parsing_errors() {
        from_str("").unwrap_err();
        from_str("{}").unwrap_err();
        from_str("42").unwrap_err();
        from_str("{").unwrap_err();
        from_str(r#"{"types": []}"#).unwrap_err();
        from_str(r#"{"rules": []}"#).unwrap_err();
        from_str(r#"{"types": [], "rules": []}"#).unwrap();
        from_str(r#"{"types": [{"id": "poi_type:foo"}], "rules": []}"#).unwrap_err();
        from_str(r#"{"types": [{"name": "bar"}], "rules": []}"#).unwrap_err();
        from_str(r#"{"types": [{"id": "poi_type:foo", "name": "bar"}], "rules": []}"#).unwrap();
    }
    #[test]
    fn check_tests() {
        from_str(
            r#"{
            "types": [
                {"id": "poi_type:bob", "name": "Bob"},
                {"id": "poi_type:bob", "name": "Bobitto"}
            ],
            "rules": []
        }"#,
        )
        .unwrap_err();
        from_str(
            r#"{
            "types": [{"id": "poi_type:bob", "name": "Bob"}],
            "rules": [
                {
                    "osm_tags_filters": [{"key": "foo", "value": "bar"}],
                    "type": "bobette"
                }
            ]
        }"#,
        )
        .unwrap_err();
    }
    #[test]
    fn check_with_colon() {
        let json = r#"{
            "types": [
                {"id": "poi_type:amenity:bicycle_rental", "name": "Station VLS"},
                {"id": "poi_type:amenity:parking", "name": "Parking"}
            ],
            "rules": [
                {
                    "osm_tags_filters": [
                        {"key": "bicycle_rental", "value": "true"}
                    ],
                    "type": "poi_type:amenity:bicycle_rental"
                },
                {
                    "osm_tags_filters": [
                        {"key": "amenity", "value": "parking:effia"}
                    ],
                    "type": "poi_type:amenity:parking"
                }
            ]
        }"#;
        let c = from_str(json).unwrap();
        assert_eq!(
            Some("poi_type:amenity:bicycle_rental"),
            c.get_poi_id(&tags(&[("bicycle_rental", "true")]))
        );
        assert_eq!(
            Some("poi_type:amenity:parking"),
            c.get_poi_id(&tags(&[("amenity", "parking:effia")]))
        );
    }
    #[test]
    fn check_all_tags_first_match() {
        let json = r#"{
            "types": [
                {"id": "poi_type:bob_titi", "name": "Bob is Bobette and Titi is Toto"},
                {"id": "poi_type:bob", "name": "Bob is Bobette"},
                {"id": "poi_type:titi", "name": "Titi is Toto"},
                {"id": "poi_type:foo", "name": "Foo is Bar"}
            ],
            "rules": [
                {
                    "osm_tags_filters": [
                        {"key": "bob", "value": "bobette"},
                        {"key": "titi", "value": "toto"}
                    ],
                    "type": "poi_type:bob_titi"
                },
                {
                    "osm_tags_filters": [
                        {"key": "bob", "value": "bobette"}
                    ],
                    "type": "poi_type:bob"
                },
                {
                    "osm_tags_filters": [
                        {"key": "titi", "value": "toto"}
                    ],
                    "type": "poi_type:titi"
                },
                {
                    "osm_tags_filters": [
                        {"key": "foo", "value": "bar"}
                    ],
                    "type": "poi_type:foo"
                }
            ]
        }"#;
        let c = from_str(json).unwrap();
        assert_eq!(
            Some("poi_type:bob"),
            c.get_poi_id(&tags(&[
                ("bob", "bobette"),
                ("titi", "tata"),
                ("foo", "bar"),
            ],))
        );
        assert_eq!(
            Some("poi_type:titi"),
            c.get_poi_id(&tags(&[
                ("bob", "bobitta"),
                ("titi", "toto"),
                ("foo", "bar"),
            ],))
        );
        assert_eq!(
            Some("poi_type:bob_titi"),
            c.get_poi_id(&tags(&[
                ("bob", "bobette"),
                ("titi", "toto"),
                ("foo", "bar"),
            ],))
        );
        assert_eq!(
            Some("poi_type:foo"),
            c.get_poi_id(&tags(&[
                ("bob", "bobitta"),
                ("titi", "tata"),
                ("foo", "bar"),
            ],))
        );
    }
}
