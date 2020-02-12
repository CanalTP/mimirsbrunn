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

use super::get_first_index_aliases;
use reqwest;
use std::path::Path;

/// Returns the total number of results in the ES
fn get_nb_elements(es_wrapper: &crate::ElasticSearchWrapper<'_>) -> u64 {
    let json = es_wrapper.search("*.*");
    json["hits"]["total"].as_u64().unwrap()
}

/// Simple call to a BANO load into ES base
/// Checks that we are able to find one object (a specific address)
pub fn bano2mimir_sample_test(es_wrapper: crate::ElasticSearchWrapper<'_>) {
    let bano2mimir = Path::new(env!("OUT_DIR"))
        .join("../../../bano2mimir")
        .display()
        .to_string();
    crate::launch_and_assert(
        &bano2mimir,
        &[
            "--input=./tests/fixtures/sample-bano.csv".into(),
            format!("--connection-string={}", es_wrapper.host()),
        ],
        &es_wrapper,
    );

    let res: Vec<_> = es_wrapper.search_and_filter("20", |_| true).collect();
    assert_eq!(res.len(), 2);

    // after an import, we should have 1 index, and some aliases to this index
    let mut res = reqwest::get(&format!("{host}/_aliases", host = es_wrapper.host())).unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);

    let json: serde_json::value::Value = res.json().unwrap();
    let raw_indexes = json.as_object().unwrap();
    let first_indexes: Vec<String> = raw_indexes.keys().cloned().collect();

    assert_eq!(first_indexes.len(), 1);
    // our index should be aliased by the master_index + an alias over the document type + dataset
    let aliases = get_first_index_aliases(raw_indexes);

    // for the moment 'munin' is hard coded, but hopefully that will change
    assert_eq!(
        aliases,
        vec!["munin", "munin_addr", "munin_addr_fr", "munin_geo_data"]
    );

    // then we import again the bano file:
    crate::launch_and_assert(
        &bano2mimir,
        &[
            "--input=./tests/fixtures/sample-bano.csv".into(),
            format!("--connection-string={}", es_wrapper.host()),
        ],
        &es_wrapper,
    );

    // we should still have only one index (but a different one)
    let mut res = reqwest::get(&format!("{host}/_aliases", host = es_wrapper.host())).unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);

    let json: serde_json::value::Value = res.json().unwrap();
    let raw_indexes = json.as_object().unwrap();
    let final_indexes: Vec<String> = raw_indexes.keys().cloned().collect();

    assert_eq!(final_indexes.len(), 1);
    assert!(final_indexes != first_indexes);

    let aliases = get_first_index_aliases(raw_indexes);
    assert_eq!(
        aliases,
        vec!["munin", "munin_addr", "munin_addr_fr", "munin_geo_data"]
    );

    // we should have imported 39 elements
    // (we shouldn't have the badly formatted line)
    let total = get_nb_elements(&es_wrapper);
    assert_eq!(total, 39);

    // We look for 'Fake-City' which should have been filtered since the street name is empty
    let res: Vec<_> = es_wrapper
        .search_and_filter("Fake-City", |_| true)
        .collect();
    assert_eq!(res.len(), 0);
}
