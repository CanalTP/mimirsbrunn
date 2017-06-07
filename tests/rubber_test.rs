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

use serde_json::value::Value;
use mimir::{Street, Admin, Coord, MimirObject};
use mimir::rubber::Rubber;
use std;
use std::cell::Cell;
use hyper;

fn check_has_elt<F: FnMut(&Value)>(es: &::ElasticSearchWrapper, mut fun: F) {
    let search = es.search("*:*"); // we get all documents in the base
    // we should have our elt
    assert_eq!(search.pointer("/hits/total"), Some(&json!(1)));
    fun(search.pointer("/hits/hits/0").unwrap());
}

fn check_has_bob(es: &::ElasticSearchWrapper) {
    let check_is_bob = |es_elt: &Value| {
        assert_eq!(es_elt.pointer("/_type").and_then(|t| t.as_str()).unwrap(),
                   "street");
        let es_bob = es_elt.pointer("/_source").unwrap();
        assert_eq!(es_bob.pointer("/id"), Some(&json!("bob")));
        assert_eq!(es_bob.pointer("/street_name"), Some(&json!("bob's street")));
        assert_eq!(es_bob.pointer("/label"), Some(&json!("bob's name")));
        assert_eq!(es_bob.pointer("/weight"), Some(&json!(0.42)));
    };
    check_has_elt(es, check_is_bob);
}

/// check the zero downtime update
/// first load a batch a data, and then upload a second one
/// during the second batch we should be able to query Elasticsearch and find the first batch
pub fn rubber_zero_downtime_test(mut es: ::ElasticSearchWrapper) {
    info!("running rubber_zero_downtime_test");
    let dataset = "my_dataset";

    let bob = Street {
        id: "bob".to_string(),
        street_name: "bob's street".to_string(),
        label: "bob's name".to_string(),
        administrative_regions: vec![],
        weight: 0.42,
        zip_codes: vec![],
        coord: Coord::new(0., 0.),
    };

    // we index our bob
    let result = es.rubber.index(dataset, std::iter::once(bob));

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 1); // we have indexed 1 element

    es.refresh(); // we need to refresh the index to be sure to get the elt;

    check_has_bob(&es);

    let bobette = Street {
        id: "bobette".to_string(),
        street_name: "bobette's street".to_string(),
        label: "bobette's name".to_string(),
        administrative_regions: vec![],
        weight: 0.24,
        zip_codes: vec![],
        coord: Coord::new(48.5110722f64, 2.68326290f64),
    };

    info!("inserting bobette");
    let mut rubber = Rubber::new(&es.docker_wrapper.host());
    // while yielding the new street, we want to check that we are still
    let checker_iter = std::iter::once(bobette).inspect(|_| {
        es.refresh(); // we send a refresh to be sure to be up to date
        check_has_bob(&es);
    });
    let result = rubber.index(dataset, checker_iter);
    assert!(result.is_ok(),
            "impossible to index bobette, res: {:?}",
            result);
    assert_eq!(result.unwrap(), 1); // we still have only indexed 1 element

    es.refresh(); // we send another refresh

    // then we should have our bobette
    let check_is_bobette = |es_elt: &Value| {
        assert_eq!(es_elt.pointer("/_type").and_then(|t| t.as_str()).unwrap(),
                   "street");
        let es_bob = es_elt.pointer("/_source").unwrap();
        assert_eq!(es_bob.pointer("/id"), Some(&json!("bobette")));
        assert_eq!(es_bob.pointer("/street_name"),
                   Some(&json!("bobette's street")));
        assert_eq!(es_bob.pointer("/label"), Some(&json!("bobette's name")));
        assert_eq!(es_bob.pointer("/weight"), Some(&json!(0.24)));

        let es_coord = es_bob.pointer("/coord").unwrap();
        assert_eq!(es_coord.pointer("/lat"), Some(&json!(48.5110722)));
        assert_eq!(es_coord.pointer("/lon"), Some(&json!(2.68326290)));

    };
    check_has_elt(&es, check_is_bobette);
}

pub fn rubber_custom_id(mut es: ::ElasticSearchWrapper) {
    info!("running rubber_custom_id");
    let dataset = "my_dataset";

    let admin = Admin {
        id: "admin:bob".to_string(),
        insee: "insee:dummy".to_string(),
        level: 8,
        name: "my admin".to_string(),
        label: "my admin (zip_code)".to_string(),
        zip_codes: vec!["zip_code".to_string()],
        weight: Cell::new(0.42),
        coord: Coord::new(48.5110722f64, 2.68326290f64),
        boundary: None,
    };

    // we index our admin
    let result = es.rubber.index(dataset, std::iter::once(admin));

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 1); // we have indexed 1 element

    es.refresh(); // we need to refresh the index to be sure to get the elt;

    let check_admin = |es_elt: &Value| {
        assert_eq!(es_elt.pointer("/_type").and_then(|t| t.as_str()).unwrap(),
                   Admin::doc_type());
        let es_source = es_elt.pointer("/_source").unwrap();
        assert_eq!(es_elt.pointer("/_id"), es_source.pointer("/id"));
        assert_eq!(es_elt.pointer("/_id"), Some(&json!("admin:bob")));
        assert_eq!(es_source.pointer("/insee"), Some(&json!("insee:dummy")));

        let es_coord = es_source.pointer("/coord").unwrap();
        assert_eq!(es_coord.pointer("/lat"), Some(&json!(48.5110722)));
        assert_eq!(es_coord.pointer("/lon"), Some(&json!(2.68326290)));
    };
    check_has_elt(&es, check_admin);
}

/// test that rubber correctly cleanup ghost indexes
/// (indexes that are not aliases to anything, for example
/// if an import has been stopped in the middle)
pub fn rubber_ghost_index_cleanup(mut es: ::ElasticSearchWrapper) {
    // we create a ghost ES index
    let client = hyper::client::Client::new();
    let old_idx_name = "munin_admin_fr_20170313_113227_006297916";
    let res =
        client.put(&format!("{host}/{idx}", host = es.host(), idx = old_idx_name)).send().unwrap();

    assert_eq!(res.status, hyper::Ok);
    info!("result: {:?}", res);

    es.refresh();
    assert_eq!(get_munin_indexes(&es), [old_idx_name.to_string()]);

    let admin = Admin {
        id: "admin:bob".to_string(),
        insee: "insee:dummy".to_string(),
        level: 8,
        name: "my admin".to_string(),
        label: "my admin (zip_code)".to_string(),
        zip_codes: vec!["zip_code".to_string()],
        weight: Cell::new(0.42),
        coord: Coord::new(48.5110722f64, 2.68326290f64),
        boundary: None,
    };

    // we index our admin
    let result = es.rubber.index("fr", std::iter::once(admin));

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 1); // we have indexed 1 element

    es.refresh(); // we need to refresh the index to be sure to get the elt;

    // we should have only one index, and it should not be the previous one
    assert_eq!(get_munin_indexes(&es).len(), 1);
    assert!(!get_munin_indexes(&es).contains(&old_idx_name.to_string()));
}

// return the list of the munin indexes
fn get_munin_indexes(es: &::ElasticSearchWrapper) -> Vec<String> {
    use super::ToJson;
    let client = hyper::client::Client::new();
    let res = client.get(&format!("{host}/_aliases", host = es.host())).send().unwrap();
    assert_eq!(res.status, hyper::Ok);

    let json = res.to_json();
    let raw_indexes = json.as_object().unwrap();
    raw_indexes.keys().cloned().collect()
}
