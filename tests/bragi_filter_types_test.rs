// Copyright © 2017, Canal TP and/or its affiliates. All rights reserved.
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
extern crate iron_test;
extern crate serde_json;
use super::get_values;
use super::BragiHandler;
use super::{count_types, get_types, get_value, to_json};
use rustless::server::status::StatusCode::{BadRequest, NotFound};

pub fn bragi_filter_types_test(es_wrapper: ::ElasticSearchWrapper) {
    let bragi = BragiHandler::new(format!("{}/munin", es_wrapper.host()));

    // ******************************************
    // we the OSM dataset, three-cities bano dataset and a stop file
    // the current dataset are thus (load order matters):
    // - osm_fixture.osm.pbf
    // - bano-three_cities
    // - stops.txt
    // ******************************************
    let osm2mimir = concat!(env!("OUT_DIR"), "/../../../osm2mimir");
    ::launch_and_assert(
        osm2mimir,
        vec![
            "--input=./tests/fixtures/osm_fixture.osm.pbf".into(),
            "--import-way".into(),
            "--import-admin".into(),
            "--import-poi".into(),
            "--level=8".into(),
            format!("--connection-string={}", es_wrapper.host()),
        ],
        &es_wrapper,
    );

    let bano2mimir = concat!(env!("OUT_DIR"), "/../../../bano2mimir");
    ::launch_and_assert(
        bano2mimir,
        vec![
            "--input=./tests/fixtures/bano-three_cities.csv".into(),
            format!("--connection-string={}", es_wrapper.host()),
        ],
        &es_wrapper,
    );

    let stops2mimir = concat!(env!("OUT_DIR"), "/../../../stops2mimir");
    ::launch_and_assert(
        stops2mimir,
        vec![
            "--input=./tests/fixtures/stops.txt".into(),
            "--dataset=dataset1".into(),
            format!("--connection-string={}", es_wrapper.host()),
        ],
        &es_wrapper,
    );

    no_type_no_dataset_test(&bragi);
    type_stop_area_no_dataset_test(&bragi);
    type_poi_and_dataset_test(&bragi);
    type_poi_and_city_no_dataset_test(&bragi);
    type_poi_and_city_with_percent_encoding_no_dataset_test(&bragi);
    type_stop_area_dataset_test(&bragi);
    unvalid_type_test(&bragi);
    addr_by_id_test(&bragi);
    admin_by_id_test(&bragi);
    street_by_id_test(&bragi);
    stop_by_id_test(&bragi);
    stop_area_that_does_not_exists(&bragi);
    type_zone_filter(&bragi);
}

fn no_type_no_dataset_test(bragi: &BragiHandler) {
    // with this query we should not find any stops
    let response = bragi.get("/autocomplete?q=Parking vélo Saint-Martin");
    let types = get_types(&response);
    let count = count_types(&types, "public_transport:stop_area");
    assert_eq!(count, 0);
}

fn type_stop_area_no_dataset_test(bragi: &BragiHandler) {
    // with this query we should return an empty response
    let response =
        bragi.get("/autocomplete?q=Parking vélo Saint-Martin&type[]=public_transport:stop_area");
    assert!(response.is_empty());
}

fn type_poi_and_dataset_test(bragi: &BragiHandler) {
    // with this query we should only find pois
    let response =
        bragi.get("/autocomplete?q=Parking vélo Saint-Martin&pt_dataset=dataset1&type[]=poi");
    let types = get_types(&response);
    assert_eq!(count_types(&types, "public_transport:stop_area"), 0);
    assert_eq!(count_types(&types, "city"), 0);
    assert_eq!(count_types(&types, "street"), 0);
    assert_eq!(count_types(&types, "house"), 0);
    assert!(count_types(&types, "poi") > 0);

    let poi = response.first().unwrap();
    assert_eq!(get_value(poi, "name"), "Parking vélo");
}

fn type_poi_and_city_no_dataset_test(bragi: &BragiHandler) {
    // with this query we should only find pois and cities
    let response = bragi.get("/autocomplete?q=melun&type[]=poi&type[]=city");
    let types = get_types(&response);
    assert_eq!(count_types(&types, "public_transport:stop_area"), 0);
    assert_eq!(count_types(&types, "street"), 0);
    assert_eq!(count_types(&types, "house"), 0);
    assert!(count_types(&types, "city") > 0);
    assert!(count_types(&types, "poi") > 0);
}

fn type_poi_and_city_with_percent_encoding_no_dataset_test(bragi: &BragiHandler) {
    // Same test as before but with percent encoded type param
    let response = bragi.get("/autocomplete?q=melun&type%5B%5D=poi&type%5B%5D=city");
    let types = get_types(&response);
    assert_eq!(count_types(&types, "public_transport:stop_area"), 0);
    assert_eq!(count_types(&types, "street"), 0);
    assert_eq!(count_types(&types, "house"), 0);
    assert!(count_types(&types, "city") > 0);
    assert!(count_types(&types, "poi") > 0);
}

fn type_stop_area_dataset_test(bragi: &BragiHandler) {
    // with this query we should only find stop areas
    let response = bragi.get(
        "/autocomplete?q=Vaux-le-Pénil&pt_dataset=dataset1&type[]=public_transport:\
         stop_area",
    );
    let types = get_types(&response);
    assert!(count_types(&types, "public_transport:stop_area") > 0);
    assert_eq!(count_types(&types, "street"), 0);
    assert_eq!(count_types(&types, "house"), 0);
    assert_eq!(count_types(&types, "city"), 0);
    assert_eq!(count_types(&types, "poi"), 0);
}

fn unvalid_type_test(bragi: &BragiHandler) {
    let response = bragi.raw_get("/autocomplete?q=melun&type[]=unvalid");
    assert!(response.is_err());

    let iron_error = response.unwrap_err();
    assert_eq!(iron_error.response.status.unwrap(), BadRequest);

    let json = to_json(iron_error.response);
    let error_msg = json.pointer("/long").unwrap().as_str().unwrap();

    assert!(error_msg.contains("unvalid is not a valid type"))
}

fn admin_by_id_test(bragi: &BragiHandler) {
    let all_20 = bragi.get("/features/admin:fr:77288");
    assert_eq!(all_20.len(), 1);
    let types = get_types(&all_20);
    let count = count_types(&types, "city");
    assert_eq!(count, 1);

    assert_eq!(get_values(&all_20, "id"), vec!["admin:fr:77288"]);
}

fn street_by_id_test(bragi: &BragiHandler) {
    let all_20 = bragi.get("/features/161162362");
    assert_eq!(all_20.len(), 1);
    let types = get_types(&all_20);

    let count = count_types(&types, "street");
    assert_eq!(count, 1);

    assert_eq!(get_values(&all_20, "id"), vec!["161162362"]);
}

fn addr_by_id_test(bragi: &BragiHandler) {
    let all_20 = bragi.get("/features/addr:2.68385;48.50539");
    assert_eq!(all_20.len(), 1);
    let types = get_types(&all_20);
    let count = count_types(&types, "house");
    assert_eq!(count, 1);
    assert_eq!(get_values(&all_20, "id"), vec!["addr:2.68385;48.50539"]);
}

fn stop_by_id_test(bragi: &BragiHandler) {
    // search with id
    let response = bragi.get("/features/stop_area:SA:second_station?pt_dataset=dataset1");
    assert_eq!(response.len(), 1);
    let stop = response.first().unwrap();
    assert_eq!(get_value(stop, "id"), "stop_area:SA:second_station");
}

fn stop_area_that_does_not_exists(bragi: &BragiHandler) {
    // search with id
    let response = bragi
        .raw_get("/features/stop_area:SA:second_station::AA?pt_dataset=dataset1")
        .unwrap();

    assert_eq!(response.status, Some(NotFound));

    let result_body = iron_test::response::extract_body_to_string(response);
    assert!(result_body.contains("Unable to find object"));
}

fn stop_area_invalid_index(bragi: &BragiHandler) {
    // if the index does not exists, we get a 404 with "impossible to find object"
    let response = bragi
        .raw_get("/features/stop_area:SA:second_station::AA?pt_dataset=invalid_dataset")
        .unwrap();

    assert_eq!(response.status, Some(NotFound));

    let result_body = iron_test::response::extract_body_to_string(response);
    assert!(result_body.contains("Impossible to find object"));
}

fn type_zone_filter(bragi: &BragiHandler) {
    // This query without the 'type=zone' filter returns cities and pois.
    // With the filter we should have only cities.
    let response_zone = bragi.get("/autocomplete?q=melun&type=zone");
    let types_zone = get_types(&response_zone);

    assert_eq!(count_types(&types_zone, "public_transport:stop_area"), 0);
    assert_eq!(count_types(&types_zone, "street"), 0);
    assert_eq!(count_types(&types_zone, "house"), 0);
    assert!(count_types(&types_zone, "city") > 0);
    assert_eq!(count_types(&types_zone, "poi"), 0);
}
