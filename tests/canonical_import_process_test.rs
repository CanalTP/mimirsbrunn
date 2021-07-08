// Copyright © 2018, Canal TP and/or its affiliates. All rights reserved.
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

#![allow(clippy::cognitive_complexity)]

use super::count_types;
use super::get_poi_type_ids;
use super::get_value;
use super::get_values;
use super::BragiHandler;
use serde_json::{self, json};
use std::path::Path;

/// Test the whole mimirsbrunn pipeline with all the import binary
/// and test thourgh bragi in the end
///
/// First we import cosmogony,
/// then openaddress (or bano),
/// then osm (without any admins)
pub fn canonical_import_process_test(es_wrapper: crate::ElasticSearchWrapper<'_>) {
    let mut bragi = BragiHandler::new(es_wrapper.host());
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

    let osm2mimir = out_dir.join("../../../osm2mimir").display().to_string();
    crate::launch_and_assert(
        &osm2mimir,
        &[
            "--input=./tests/fixtures/osm_fixture.osm.pbf".into(),
            "--config-dir=./config".into(),
            "--import-way=true".into(),
            "--import-poi=true".into(),
            format!("--connection-string={}", es_wrapper.host()),
        ],
        &es_wrapper,
    );

    melun_test(&mut bragi);
    lang_test(&mut bragi);
    invalid_parameter_autocomplete_test(&mut bragi);
    wrong_shape_test(&mut bragi);
    invalid_type_test(&mut bragi);
    invalid_route_test(&mut bragi);
    invalid_coord_test(&mut bragi);
    valid_timeout_test(&mut bragi);
    filter_zone_type_test(&mut bragi);
    zone_filter_error_message_test(&mut bragi);
}

fn melun_test(bragi: &mut BragiHandler) {
    let all_melun = bragi.get("/autocomplete?q=Melun");
    let types = get_values(&all_melun, "zone_type");
    let count = count_types(&types, "city");
    assert_eq!(count, 1);

    // and the city should be the first result
    let melun = all_melun.first().unwrap();
    assert_eq!(melun["name"], "Melun");
    // the fact that we have this label proves that this admins comes
    // from cosmogony as we cannot have this label through osm alone
    assert_eq!(
        melun["label"],
        "Melun (77000-CP77001), Fausse Seine-et-Marne, France hexagonale"
    );
    assert_eq!(melun["postcode"], "77000;77003;77008;CP77001");
    assert_eq!(melun["id"], "admin:osm:relation:80071");
    assert_eq!(melun["zone_type"], "city");
    assert_eq!(melun["city"], serde_json::Value::Null);
    assert_eq!(
        melun["bbox"],
        json!(vec![2.628_466_9, 48.523_525_9, 2.682_018_4, 48.560_761_6])
    );
    let admins = melun["administrative_regions"]
        .as_array()
        .expect("admins must be array");
    assert_eq!(admins.len(), 2, "{:?}", admins);

    // we should also find other object that are in the city
    // (in the data there is at least one poi, and one street that matches)
    let cityhall = all_melun
        .iter()
        .find(|e| get_value(e, "name") == "Hôtel de Ville")
        .unwrap();
    // the poi should be geocoded and be linked to melun and it's administrative hierarchy
    assert_eq!(cityhall["city"], "Melun");
    assert_eq!(cityhall["label"], "Hôtel de Ville (Melun)");
    assert_eq!(cityhall["postcode"], "77000");
    assert_eq!(cityhall["citycode"], "77288");
    assert_eq!(get_value(cityhall, "type"), "poi");
    assert_eq!(get_poi_type_ids(cityhall), &["poi_type:amenity:townhall"]);
    assert_eq!(cityhall["city"], "Melun");

    let cityhall_admins = cityhall["administrative_regions"]
        .as_array()
        .expect("admins must be array");

    assert_eq!(cityhall_admins.len(), 3);
    assert_eq!(cityhall_admins[0]["id"], "admin:osm:relation:80071");
    assert_eq!(cityhall_admins[0]["insee"], "77288");
    assert_eq!(
        cityhall_admins[0]["label"],
        "Melun (77000-CP77001), Fausse Seine-et-Marne, France hexagonale"
    );
    assert_eq!(cityhall_admins[0]["name"], "Melun");
    assert_eq!(cityhall_admins[0]["zone_type"], "city");

    // i18n labels and names have been cleaned up
    assert_eq!(cityhall_admins[0].get("labels"), None);
    assert_eq!(cityhall_admins[0].get("names"), None);

    assert_eq!(cityhall_admins[1]["id"], "admin:osm:relation:424253843");
    assert_eq!(cityhall_admins[1]["insee"], "77");
    assert_eq!(
        cityhall_admins[1]["label"],
        "Fausse Seine-et-Marne, France hexagonale"
    );
    assert_eq!(cityhall_admins[1]["name"], "Fausse Seine-et-Marne");
    assert_eq!(cityhall_admins[1]["zone_type"], "state_district");

    assert_eq!(cityhall_admins[2]["id"], "admin:osm:relation:424256272");
    assert_eq!(cityhall_admins[2]["label"], "France hexagonale");
    assert_eq!(cityhall_admins[2]["name"], "France hexagonale");
    assert_eq!(cityhall_admins[2]["zone_type"], "country");
    assert_eq!(
        cityhall_admins[2]["codes"],
        json!([
            {"name": "ISO3166-1", "value": "FR"},
            {"name": "ISO3166-1:alpha2", "value": "FR"},
            {"name": "ISO3166-1:alpha3", "value": "FRA"},
            {"name": "ISO3166-1:numeric", "value": "250"},
            {"name": "wikidata", "value": "Q142"},
        ])
    );

    // the poi should have been associated to an address
    let poi_addr = cityhall["address"].as_object().unwrap();

    assert_eq!(poi_addr["label"], "2 Rue de la Reine Blanche (Melun)");
    assert_eq!(poi_addr["housenumber"], "2");
    assert_eq!(poi_addr["street"], "Rue de la Reine Blanche");
    assert_eq!(poi_addr["postcode"], "77288");
    assert_eq!(poi_addr["city"], "Melun");
}

fn lang_test(bragi: &mut BragiHandler) {
    let all_francia = bragi.get("/autocomplete?q=Francia&lang=es");
    let result = all_francia.first().unwrap();
    assert_eq!(result["name"], "Francia");
    assert_eq!(result["zone_type"], "country");
    assert_eq!(result["label"], "Francia");

    let all_melun = bragi.get("/autocomplete?q=Melun&lang=es");
    let result = all_melun.first().unwrap();
    assert_eq!(result["name"], "Melun");
    assert_eq!(result["zone_type"], "city");
    assert_eq!(
        result["label"],
        "Melun (77000-CP77001), Sena y Marne, Francia"
    );

    let all_cityhall = bragi.get("/autocomplete?q=Hotel+de+ville+melun&lang=es");
    let result = all_cityhall.first().unwrap();
    assert_eq!(result["name"], "Hôtel de Ville");
    assert_eq!(result["label"], "Hôtel de Ville (Melun)");
    let admins = result["administrative_regions"]
        .as_array()
        .expect("admins must be array");
    let country = admins
        .iter()
        .find(|a| a["zone_type"] == "country")
        .expect("POI should have a country among all admins");
    assert_eq!(country["name"], "Francia");
    assert_eq!(country["label"], "Francia");
    let city = admins
        .iter()
        .find(|a| a["zone_type"] == "city")
        .expect("POI should have a city among admins");
    assert_eq!(city["name"], "Melun");
    assert_eq!(
        city["label"],
        "Melun (77000-CP77001), Sena y Marne, Francia"
    );

    // Multiple 'lang' causes 400
    let r = bragi.get_unchecked_json("/autocomplete?q=Melun&lang=es&lang=fr");

    assert_eq!(
        r,
        (
            actix_web::http::StatusCode::BAD_REQUEST,
            json!({
                "short": "validation error",
                "long": "invalid argument: failed with reason: Multiple values for one key",
            })
        )
    );
}

pub fn bragi_invalid_es_test(_es_wrapper: crate::ElasticSearchWrapper<'_>) {
    let mut bragi = BragiHandler::new("http://invalid_es_url/munin".to_string());

    // the status does not check the ES connexion, so for the status all is good
    let (status, _) = bragi.raw_get("/status");
    assert_eq!(status, actix_web::http::StatusCode::OK);

    // the autocomplete gives a 503
    let r = bragi.get_unchecked_json("/autocomplete?q=toto");

    assert_eq!(
        r,
        (
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            json!({
                "short": "query error",
                "long": "service unavailable",
            })
        )
    );
}

fn invalid_parameter_autocomplete_test(bragi: &mut BragiHandler) {
    // if a param is not correct, we should have a nice error
    // this error could be better, but that will do for the moment
    assert_eq!(
        bragi.get_unchecked_json("/autocomplete?limit=limit_should_be_an_int"),
        (
            actix_web::http::StatusCode::BAD_REQUEST,
            json!({
                "short": "validation error",
                "long": "invalid argument: failed with reason: invalid digit found in string",
            })
        )
    );
}

fn invalid_type_test(bragi: &mut BragiHandler) {
    assert_eq!(
        bragi.get_unchecked_json("/autocomplete?q=a&type[]=invalid_type"),
        (
            actix_web::http::StatusCode::BAD_REQUEST,
            json!({
                "long": "invalid argument: failed with reason: unknown variant `invalid_type`, expected one of `city`, `house`, `poi`, `public_transport:stop_area`, `street`, `zone`",
                "short": "validation error"
            })
        )
    );
}

fn invalid_route_test(bragi: &mut BragiHandler) {
    assert_eq!(
        bragi.get_unchecked_json("/invalid_route"),
        (
            actix_web::http::StatusCode::NOT_FOUND,
            json!({
                "long": "route '/invalid_route' does not exists",
                "short": "no route"
            })
        )
    );
}

fn wrong_shape_test(bragi: &mut BragiHandler) {
    // The shape should be a valid geojson object
    // there, the shape has no 'property' field
    let shape = r#"{"shape":{"type":"Feature","geometry":{"type":"Polygon",
        "coordinates":[[[2.376488, 48.846431],
        [2.376306, 48.846430],[2.376309, 48.846606],[ 2.376486, 48.846603], [2.376488, 48.846431]]]}}}"#;
    let (status, r) = bragi.raw_post("/autocomplete?q=15 Rue Hector Malot, (Paris)", shape);

    assert_eq!(status, actix_web::http::StatusCode::BAD_REQUEST);

    assert_eq!(
        bragi.as_json(r),
        json!({
            "short": "validation error",
            "long": "invalid json: Json deserialize error: Expected GeoJSON property \'properties\'. at line 3 column 102",
        })
    );
}

fn invalid_coord_test(bragi: &mut BragiHandler) {
    assert_eq!(
        bragi.get_unchecked_json("/autocomplete?q=a&lat=12"),
        (
            actix_web::http::StatusCode::BAD_REQUEST,
            json!({
                "long": "Invalid parameter: you should provide a 'lon' AND a 'lat' parameter if you provide one of them",
                "short": "validation error"
            })
        )
    );
    // if we give an invalid type we get an error
    // this error could be more explicit (name of the field at least), but that will be for later
    assert_eq!(
        bragi.get_unchecked_json("/autocomplete?q=a&lat=a&lon=12"),
        (
            actix_web::http::StatusCode::BAD_REQUEST,
            json!({
                "long": "invalid argument: failed with reason: invalid float literal",
                "short": "validation error"
            })
        )
    );
    // we check that the lat/lon are valid latitude
    assert_eq!(
        bragi.get_unchecked_json("/autocomplete?q=a&lat=12&lon=9999"),
        (
            actix_web::http::StatusCode::BAD_REQUEST,
            json!({
                "long": "Invalid parameter: lon is not a valid longitude",
                "short": "validation error"
            })
        )
    );
    assert_eq!(
        bragi.get_unchecked_json("/autocomplete?q=a&lat=-1000&lon=-12"),
        (
            actix_web::http::StatusCode::BAD_REQUEST,
            json!({
                "long": "Invalid parameter: lat is not a valid latitude",
                "short": "validation error"
            })
        )
    );
}

// we just check that the timeout is correctly parser
fn valid_timeout_test(bragi: &mut BragiHandler) {
    let _ = bragi.get("/autocomplete?q=france&timeout=12340");
}

fn filter_zone_type_test(bragi: &mut BragiHandler) {
    let geocodings = bragi.get("/autocomplete?q=France&type[]=zone&zone_type[]=state_district");
    let types = get_values(&geocodings, "zone_type");
    assert_eq!(count_types(&types, "state_district"), 1);

    let geocodings = bragi.get("/autocomplete?q=France&type[]=zone&zone_type[]=country");
    let types = get_values(&geocodings, "zone_type");
    assert_eq!(count_types(&types, "country"), 1);

    let geocodings = bragi
        .get("/autocomplete?q=France&type[]=zone&zone_type[]=state_district&zone_type[]=country");
    let types = get_values(&geocodings, "zone_type");
    assert_eq!(count_types(&types, "state_district"), 1);
    assert_eq!(count_types(&types, "country"), 1);

    // all terms in query should match
    let geocodings = bragi.get("/autocomplete?q=Melun%20France&type[]=zone&zone_type[]=city");
    assert_eq!(geocodings.len(), 1);
    assert_eq!(get_values(&geocodings, "zone_type"), &["city"]);

    // zone_type and poi_type should be applied separately to zones and pois
    let geocodings = bragi.get("/autocomplete?q=Melun%20France&type[]=zone&type[]=poi&zone_type[]=city&poi_type[]=amenity:college");
    assert_eq!(geocodings.len(), 1);
    assert_eq!(get_values(&geocodings, "zone_type"), &["city"]);
}

fn zone_filter_error_message_test(bragi: &mut BragiHandler) {
    let geocodings =
        bragi.get_unchecked_json("/autocomplete?q=France&type[]=poi&zone_type[]=country");
    assert_eq!(
        geocodings,
        (
            actix_web::http::StatusCode::BAD_REQUEST,
            json!({
                "short": "validation error",
                "long": "Invalid parameter: zone_type[] parameter requires to have 'type[]=zone'",
            })
        )
    );
}
