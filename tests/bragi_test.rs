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

extern crate bragi;
extern crate rustless;
extern crate iron;
extern crate iron_test;
extern crate serde_json;
use std::process::Command;
extern crate mime;
use std::collections::BTreeMap;

fn get_handler(url: String) -> rustless::Application {
    let api = bragi::api::ApiEndPoint { es_cnx_string: url }.root();
    rustless::Application::new(api)
}

pub fn bragi_tests(es_wrapper: ::ElasticSearchWrapper) {

    let bano2mimir = concat!(env!("OUT_DIR"), "/../../../bano2mimir");
    info!("Launching {}", bano2mimir);
    let status = Command::new(bano2mimir)
                     .args(&["--input=./tests/fixtures/sample-bano.csv".into(),
                             format!("--connection-string={}", es_wrapper.host())])
                     .status()
                     .unwrap();
    assert!(status.success(), "`bano2mimir` failed {}", &status);
    es_wrapper.refresh();

    let handler = get_handler(format!("{}/munin", es_wrapper.host()));

    let bragi_get = |q| {
        iron_test::request::get(&format!("http://localhost:3000{}", q),
                                iron::Headers::new(),
                                &handler)
            .unwrap()
    };

    let bragi_params_validation = |q| {
        iron_test::request::get(&format!("http://localhost:3000{}", q),
                                iron::Headers::new(),
                                &handler)
    };

    let bragi_post_shape = |q, shape| {
        let mut header = iron::Headers::new();
        let mime: mime::Mime = "application/json".parse().unwrap();
        header.set(iron::headers::ContentType(mime));

        iron_test::request::post(&format!("http://localhost:3000{}", q),
                                 header,
                                 shape,
                                 &handler)
            .unwrap()
    };

    let to_json = |r| -> serde_json::Value {
        let s = iron_test::response::extract_body_to_string(r);
        serde_json::from_str(&s).unwrap()
    };

    let get_results = |r| -> Vec<_> {
        to_json(r)
            .find("features")
            .expect("wrongly formated bragi response")
            .as_array()
            .expect("features must be array")
            .iter()
            .map(|f| {
                f.pointer("/properties/geocoding")
                 .expect("no geocoding object in bragi response")
                 .as_object()
                 .unwrap()
                 .clone()
            })
            .collect()
    };
    // Call status
    let resp = bragi_get("/status");
    let result_body = iron_test::response::extract_body_to_string(resp);

    assert_eq!(result_body,
               r#"{"version":"1.2.0","es":"http://localhost:9242/munin","status":"good"}"#);

    // Call autocomplete
    let resp = bragi_get("/autocomplete?q=15 Rue Hector Malot, (Paris)");
    let result_body = iron_test::response::extract_body_to_string(resp);
    let result = concat!(r#"{"type":"FeatureCollection","#,
                         r#""geocoding":{"version":"0.1.0","query":""},"#,
                         r#""features":[{"type":"Feature","geometry":{"coordinates":"#,
                         r#"[2.376379,48.846495],"type":"Point"},"#,
                         r#""properties":{"geocoding":{"id":"addr:2.376379;48.846495","#,
                         r#""type":"house","label":"15 Rue Hector Malot (Paris)","#,
                         r#""name":"15 Rue Hector Malot","housenumber":"15","#,
                         r#""street":"Rue Hector Malot","postcode":"75012","#,
                         r#""city":null,"administrative_regions":[]}}}]}"#);
    assert_eq!(result_body, result);

    // A(48.846431 2.376488)
    // B(48.846430 2.376306)
    // C(48.846606 2.376309)
    // D(48.846603 2.376486)
    // R(48.846495 2.376378) : 15 Rue Hector Malot, (Paris)
    // E(48.846452 2.376580) : 18 Rue Hector Malot, (Paris)


    //             E
    //
    //      A ---------------------D
    //      |                      |
    //      |         R            |
    //      |                      |
    //      |                      |
    //      B ---------------------C
    // Search with shape where house number in shape
    let shape = r#"{"geometry":{"type":"Polygon","coordinates":[[[2.376488, 48.846431],[2.376306, 48.846430],[2.376309, 48.846606],[ 2.376486, 48.846603]]]}}"#;
    let resp = bragi_post_shape("/autocomplete?q=15 Rue Hector Malot, (Paris)", shape);

    let result_body = iron_test::response::extract_body_to_string(resp);
    let result = concat!(r#"{"type":"FeatureCollection","#,
                         r#""geocoding":{"version":"0.1.0","query":""},"#,
                         r#""features":[{"type":"Feature","geometry":{"coordinates":"#,
                         r#"[2.376379,48.846495],"type":"Point"},"#,
                         r#""properties":{"geocoding":{"id":"addr:2.376379;48.846495","#,
                         r#""type":"house","label":"15 Rue Hector Malot (Paris)","#,
                         r#""name":"15 Rue Hector Malot","housenumber":"15","#,
                         r#""street":"Rue Hector Malot","postcode":"75012","#,
                         r#""city":null,"administrative_regions":[]}}}]}"#);
    assert_eq!(result_body, result);

    // Search with shape where house number out shape
    let resp = bragi_post_shape("/autocomplete?q=18 Rue Hector Malot, (Paris)", shape);
    let result_body = iron_test::response::extract_body_to_string(resp);
    let result = concat!(r#"{"type":"FeatureCollection","geocoding":{"version":"0.1.0","query":""},"features":[]}"#);
    assert_eq!(result_body, result);

    // test with a lon/lat
    // in the dataset there are 2 '20 rue hector malot', one in paris and one in trifouilli-les-Oies
    let all_20 = get_results(bragi_get("/autocomplete?q=20 rue hect mal")); // in the mean time we time our prefix search_query
    assert_eq!(all_20.len(), 2);
    // the first one is paris
    // TODO uncomment this test, for the moment since osm is not loaded, the order is random
    // assert_eq!(get_labels(&all_20), vec!["20 Rue Hector Malot (Paris)", "20 Rue Hector Malot (Trifouilli-les-Oies)"]);

    // if we give a lon/lat near trifouilli-les-Oies, we'll have another sort
    let all_20 = get_results(bragi_get("/autocomplete?q=20 rue hector malot&lat=50.2&lon=2.0"));
    assert_eq!(get_labels(&all_20),
               vec!["20 Rue Hector Malot (Trifouilli-les-Oies)", "20 Rue Hector Malot (Paris)"]);
    // and when we're in paris, we get paris first
    let all_20 = get_results(bragi_get("/autocomplete?q=20 rue hector malot&lat=48&lon=2.4"));
    assert_eq!(get_labels(&all_20),
               vec!["20 Rue Hector Malot (Paris)", "20 Rue Hector Malot (Trifouilli-les-Oies)"]);


    // Search by zip_codes
    let osm2mimir = concat!(env!("OUT_DIR"), "/../../../osm2mimir");
    info!("Launching {}", osm2mimir);
    ::launch_and_assert(osm2mimir,
                        vec!["--input=./tests/fixtures/three_cities.osm.pbf".into(),
                             "--import-way".into(),
                             "--level=8".into(),
                             format!("--connection-string={}", es_wrapper.host())],
                        &es_wrapper);
    let all_20 = get_results(bragi_get("/autocomplete?q=77000"));
    assert_eq!(all_20.len(), 10);
    assert!(get_postcodes(&all_20).iter().all(|r| *r == "77000"));

    let types = get_types(&all_20);
    let count = count_types(&types, "street");
    assert_eq!(count, 7);

    let count = count_types(&types, "city");
    assert_eq!(count, 3);

    let count = count_types(&types, "house");
    assert_eq!(count, 0);

    // zip_code and name of street
    let all_20 = get_results(bragi_get("/autocomplete?q=77000 Lotissement le Clos de Givry"));
    assert_eq!(all_20.len(), 1);
    assert!(get_postcodes(&all_20).iter().all(|r| *r == "77000"));

    let types = get_types(&all_20);
    let count = count_types(&types, "street");
    assert_eq!(count, 1);

    let count = count_types(&types, "city");
    assert_eq!(count, 0);

    let count = count_types(&types, "house");
    assert_eq!(count, 0);

    // zip_code and name of admin
    let all_20 = get_results(bragi_get("/autocomplete?q=77000 Vaux-le-Pénil"));
    assert_eq!(all_20.len(), 4);
    assert!(get_postcodes(&all_20).iter().all(|r| *r == "77000"));
    let types = get_types(&all_20);
    let count = count_types(&types, "street");
    assert_eq!(count, 3);

    let count = count_types(&types, "city");
    assert_eq!(count, 1);

    let count = count_types(&types, "house");
    assert_eq!(count, 0);

    // zip_code on addr
    let bano2mimir = concat!(env!("OUT_DIR"), "/../../../bano2mimir");
    info!("Launching {}", bano2mimir);
    ::launch_and_assert(bano2mimir,
                        vec!["--input=./tests/fixtures/bano-three_cities.csv".into(),
                             format!("--connection-string={}", es_wrapper.host())],
                        &es_wrapper);

    let all_20 = get_results(bragi_get("/autocomplete?q=77255"));
    assert_eq!(all_20.len(), 1);
    assert!(get_postcodes(&all_20).iter().all(|r| *r == "77255"));
    let types = get_types(&all_20);
    let count = count_types(&types, "street");
    assert_eq!(count, 0);

    let count = count_types(&types, "city");
    assert_eq!(count, 0);

    let count = count_types(&types, "house");
    assert_eq!(count, 1);

    // zip_code and name of addr
    let all_20 = get_results(bragi_get("/autocomplete?q=77288 Rue de la Reine Blanche"));
    assert_eq!(all_20.len(), 1);
    assert!(get_postcodes(&all_20).iter().all(|r| *r == "77288"));
    let types = get_types(&all_20);
    let count = count_types(&types, "street");
    assert_eq!(count, 0);

    let count = count_types(&types, "city");
    assert_eq!(count, 0);

    let count = count_types(&types, "house");
    assert_eq!(count, 1);

    assert_eq!(get_labels(&all_20),
               vec!["2 Rue de la Reine Blanche (Melun)"]);

    //
    //      A ---------------------D
    //      |                      |
    //      |        === street    |
    //      |                      |
    //      |                      |
    //      B ---------------------C
    // Search with shape where street in shape
    let shape = r#"{"geometry":{"type":"Polygon","coordinates":[[[2.656546, 48.537227],[2.657608, 48.537244],[2.656476, 48.536545],[2.657340, 48.536602]]]}}"#;

    let geocodings = get_results(bragi_post_shape("/autocomplete?q=Rue du Port", shape));
    assert_eq!(geocodings.len(), 1);
    assert_eq!(get_labels(&geocodings), vec!["Rue du Port (Melun)"]);

    //
    //      A ---------------------D
    //      |                      |
    //      |                      | === street
    //      |                      |
    //      |                      |
    //      B ---------------------C
    // Search with shape where street outside shape
    let shape = r#"{"geometry":{"type":"Polygon","coordinates":[[[2.656546, 68.537227],[2.657608, 68.537244],[2.656476, 68.536545],[2.657340, 68.536602]]]}}"#;

    let geocodings = get_results(bragi_post_shape("/autocomplete?q=Rue du Port", shape));
    assert_eq!(geocodings.len(), 0);

    //
    //      A ---------------------D
    //      |                      |
    //      |        X Melun       |
    //      |                      |
    //      |                      |
    //      B ---------------------C
    // Search with shape where admin in shape
    let shape = r#"{"geometry":{"type":"Polygon","coordinates":[[[2.656546, 48.538927],[2.670816, 48.538927],[2.676476, 48.546545],[2.656546, 48.546545]]]}}"#;

    let geocodings = get_results(bragi_post_shape("/autocomplete?q=Melun", shape));
    assert_eq!(geocodings.len(), 1);
    assert_eq!(get_labels(&geocodings), vec!["Melun"]);

    //
    //      A ---------------------D
    //      |                      |
    //      |                      | X Melun
    //      |                      |
    //      |                      |
    //      B ---------------------C
    // Search with shape where admin outside shape
    let shape = r#"{"geometry":{"type":"Polygon","coordinates":[[[2.656546, 66.538927],[2.670816, 68.538927],[2.676476, 68.546545],[2.656546, 68.546545]]]}}"#;

    let geocodings = get_results(bragi_post_shape("/autocomplete?q=Melun", shape));
    assert_eq!(geocodings.len(), 0);

    // Test POIs
    ::launch_and_assert(osm2mimir,
                        vec!["--input=./tests/fixtures/three_cities.osm.pbf".into(),
                             "--import-poi".into(),
                             "--level=8".into(),
                             format!("--connection-string={}", es_wrapper.host())],
                        &es_wrapper);
    let geocodings = get_results(bragi_get("/autocomplete?q=Le-Mée-sur-Seine Courtilleraies"));
    let types = get_types(&geocodings);
    let count = count_types(&types, "poi");
    assert_eq!(count, 1);
    assert_eq!(get_labels(&geocodings), vec!["Le-Mée-sur-Seine Courtilleraies"]);
    assert!(get_postcodes(&geocodings).iter().all(|r| *r == "77350"));


    let geocodings = get_results(bragi_get("/autocomplete?q=Melun Rp"));
    let types = get_types(&geocodings);
    let count = count_types(&types, "poi");
    assert_eq!(count, 2);
    assert!(get_labels(&geocodings).contains(&"Melun Rp (Melun)"));
    assert!(get_postcodes(&geocodings).iter().all(|r| *r == "77000"));

    // search by zip code
    let geocodings = get_results(bragi_get("/autocomplete?q=77000&limit=15"));
    let types = get_types(&geocodings);
    assert_eq!(count_types(&types, "poi"), 2);
    assert_eq!(count_types(&types, "city"), 3);
    assert_eq!(count_types(&types, "street"), 7);

    // search by zip code and limit is string type
    let geocodings = bragi_params_validation("/autocomplete?q=77000&limit=ABCD");
    assert!(geocodings.is_err() , true);

    // search by zip code and limit < 0
    let geocodings = bragi_params_validation("/autocomplete?q=77000&limit=-1");
    assert!(geocodings.is_err() , true);

    // search by zip code and limit and offset
    let all_20 = get_results(bragi_get("/autocomplete?q=77000&limit=10&offset=0"));
    assert_eq!(all_20.len(), 10);

    let all_20 = get_results(bragi_get("/autocomplete?q=77000&limit=10&offset=10"));
    assert_eq!(all_20.len(), 2);

    // search poi: Poi is relation in osm data
    let geocodings = get_results(bragi_get("/autocomplete?q=Parking (Le Coudray-Montceaux)"));
    let types = get_types(&geocodings);
    assert_eq!(count_types(&types, "poi"), 1);
    
    // search poi: Poi is way in osm data
    let geocodings = get_results(bragi_get("/autocomplete?q=77000 Hôtel de Ville (Melun)"));
    let types = get_types(&geocodings);
    assert_eq!(count_types(&types, "poi"), 1);
}

fn get_labels<'a>(r: &'a Vec<BTreeMap<String, serde_json::Value>>) -> Vec<&'a str> {
    r.iter().map(|e| e.get("label").and_then(|l| l.as_str()).unwrap_or("")).collect()
}

fn get_postcodes<'a>(r: &'a Vec<BTreeMap<String, serde_json::Value>>) -> Vec<&'a str> {
    r.iter().map(|e| e.get("postcode").and_then(|l| l.as_str()).unwrap_or("")).collect()
}

fn get_types<'a>(r: &'a Vec<BTreeMap<String, serde_json::Value>>) -> Vec<&'a str> {
    r.iter().map(|e| e.get("type").and_then(|l| l.as_str()).unwrap_or("")).collect()
}

fn count_types(types: &Vec<&str>, value: &str) -> usize {
    types.iter().filter(|&t| *t == value).count()
}
