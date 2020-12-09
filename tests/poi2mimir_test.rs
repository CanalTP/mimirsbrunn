// Copyright © 2019, Canal TP and/or its affiliates. All rights reserved.
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

use serde_json::value::Value;
use std::path::Path;

/// Inserts POI into Elasticsearch and check results.
/// Prior to POI insertion, we need to insert contextual data:
/// - cosmogony (so that we get the admins),
/// - bano (so that we can attach the POI to an address)
pub fn poi2mimir_sample_test(es_wrapper: crate::ElasticSearchWrapper<'_>) {
    let out_dir = Path::new(env!("OUT_DIR"));

    let cosmogony2mimir = out_dir
        .join("../../../cosmogony2mimir")
        .display()
        .to_string();
    crate::launch_and_assert(
        &cosmogony2mimir,
        &[
            "--input=./tests/fixtures/cosmogony.json".into(),
            "--lang=fr".into(),
            "--lang=es".into(),
            format!("--connection-string={}", es_wrapper.host()),
        ],
        &es_wrapper,
    );

    let bano2mimir = out_dir.join("../../../bano2mimir").display().to_string();
    crate::launch_and_assert(
        &bano2mimir,
        &[
            "--input=./tests/fixtures/bano-three_cities.csv".into(),
            format!("--connection-string={}", es_wrapper.host()),
        ],
        &es_wrapper,
    );

    // Now import some POI. We will assume this is a private dataset, so we pass
    // the private flag to poi2mimir.
    let poi2mimir = out_dir.join("../../../poi2mimir").display().to_string();
    crate::launch_and_assert(
        &poi2mimir,
        &[
            "--input=./tests/fixtures/poi/test.poi".into(),
            format!("--connection-string={}", es_wrapper.host()),
            "--dataset=mti".into(),
            "--private".into(),
        ],
        &es_wrapper,
    );

    // Now we'll make sure that the 'munin_poi_mti_{timestamp}' index is aliased by only one alias,
    // Namely 'munin_poi_mti'. So we'll get all the aliases.
    // For reference, aliases should look like
    //
    // {                                   |
    //   "munin_admin_fr_[timestamp]": {   |
    //     "aliases": {                    |
    //       "munin": {},                  |
    //       "munin_admin": {},            | ==> Added by cosmogony2mimir
    //       "munin_admin_fr": {},         |
    //       "munin_geo_data": {}          |
    //     }                               |
    //   },                              ==
    //   "munin_addr_fr_[timestamp]": {    |
    //     "aliases": {                    |
    //       "munin": {},                  |
    //       "munin_addr": {},             | ==> Added by bano2mimir
    //       "munin_addr_fr": {},          |
    //       "munin_geo_data": {}          |
    //     }                               |
    //   },                              ==
    //   "munin_poi_mti_[timestamp]": {    |
    //     "aliases": {                    |
    //       "munin_poi_mti": {}           | ==> Added by poi2mimir
    //     }                               |
    //   }                                 |
    // }
    let res =
        reqwest::blocking::get(&format!("{host}/_aliases", host = es_wrapper.host())).unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);

    let json: Value = res.json().unwrap();
    let raw_indexes = json.as_object().unwrap();

    let first_indexes: Vec<String> = raw_indexes.keys().cloned().collect();

    let raw_poi_index = first_indexes
        .iter()
        .find(|index| index.starts_with("munin_poi_mti"))
        .unwrap();

    // The result of raw_indexes[raw_poi_index] is an object that looks like:
    // Object({"aliases": Object({"munin_poi_mti": Object({})})})
    // So we turn it into a map, get the 'aliases' value,
    // turn the result into a map, and get the key (hopefully its unique).
    let poi_index_alias = raw_indexes
        .get(raw_poi_index)
        .and_then(|json| json.as_object())
        .and_then(|s| s.get("aliases"))
        .and_then(|json| json.as_object())
        .and_then(|s| s.keys().cloned().next())
        .unwrap_or_else(String::new);

    assert_eq!(poi_index_alias, "munin_poi_mti");

    // Now that we're sure we're hitting the munin_poi_mti index, count how many documents we have
    // in there. This should be the same number of POI as in the test.poi file we inserted.
    assert_eq!(es_wrapper.count("munin_poi_mti", "_type:poi"), 4);

    // Ok, now check that we can get a POI on that index
    let agence_du_four = es_wrapper
        .search_and_filter_on_index("munin_poi_mti", "label:Agence TCL Du Four", |place| {
            place.label().starts_with("Agence TCL Du Four à Chaux")
        })
        .next()
        .expect("Could not find Agence TCL Du Four");
    assert!(agence_du_four.is_poi());

    // We test that the POI that was close to an address has been 'attached' to that address,
    // and inherited its admin and address: (Note that the data have been manipulated to fit)
    assert_eq!(
        agence_du_four
            .admins()
            .iter()
            .filter(|adm| adm.is_city())
            .map(|adm| &adm.name)
            .collect::<Vec<_>>(),
        vec!["Livry-sur-Seine"]
    );

    // If the POI has a city admin, then its weight is that of the city (or at least != 0.0)
    assert_relative_ne!(
        agence_du_four.poi().unwrap().weight,
        0.0f64,
        epsilon = std::f64::EPSILON
    );

    // Make sure that that POI is not visible from the global index
    assert!(es_wrapper
        .search_and_filter("label:Agence TCL Du Four", |place| {
            place.label().starts_with("Agence TCL Du Four à Chaux")
        })
        .next()
        .is_none());
}
