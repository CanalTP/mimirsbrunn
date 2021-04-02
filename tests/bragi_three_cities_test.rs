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

use super::count_types;
use super::get_types;
use super::get_value;
use super::get_values;
use super::BragiHandler;
use std::path::Path;

pub fn bragi_three_cities_test(es_wrapper: crate::ElasticSearchWrapper<'_>) {
    let mut bragi = BragiHandler::new(es_wrapper.host());
    let out_dir = Path::new(env!("OUT_DIR"));

    // *********************************
    // We load the OSM dataset and three-cities bano dataset
    // the current dataset are thus (load order matters):
    // - osm_fixture.osm.pbf (including ways)
    // - bano-three_cities
    // *********************************
    let osm2mimir = out_dir.join("../../../osm2mimir").display().to_string();
    crate::launch_and_assert(
        &osm2mimir,
        &[
            "--input=./tests/fixtures/osm_fixture.osm.pbf".into(),
            "--config-dir=./config".into(),
            "--import-admin=true".into(),
            "--import-way=true".into(),
            "--level=8".into(),
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

    let stops2mimir = out_dir.join("../../../stops2mimir").display().to_string();
    crate::launch_and_assert(
        &stops2mimir,
        &[
            "--input=./tests/fixtures/stops_shape.txt".into(),
            format!("--connection-string={}", es_wrapper.host()),
        ],
        &es_wrapper,
    );

    three_cities_housenumber_zip_code_test(&mut bragi);
    three_cities_zip_code_test(&mut bragi);
    three_cities_zip_code_address_test(&mut bragi);
    three_cities_shape_test(&mut bragi);
}

fn three_cities_housenumber_zip_code_test(bragi: &mut BragiHandler) {
    // we search for a house number with a postcode, we should be able to find
    // the house number with this number in this city
    let all_20 = bragi.get("/autocomplete?q=3 rue 77255");
    assert_eq!(all_20.len(), 1);
    assert!(get_values(&all_20, "postcode")
        .iter()
        .all(|r| *r == "77255",));
    let types = get_types(&all_20);
    let count = count_types(&types, "street");
    assert_eq!(count, 0);

    let count = count_types(&types, "city");
    assert_eq!(count, 0);

    let count = count_types(&types, "house");
    assert_eq!(count, 1);
    let first_house = all_20.iter().find(|e| get_value(e, "type") == "house");
    assert_eq!(get_value(first_house.unwrap(), "citycode"), "77255");
    assert_eq!(
        get_value(first_house.unwrap(), "label"),
        "3 Rue du Four à Chaux (Livry-sur-Seine)"
    );
}

fn three_cities_zip_code_test(bragi: &mut BragiHandler) {
    // we query with only a zip code, we should be able to find admins,
    // and some street of it (and all on this admin)
    let res = bragi.get("/autocomplete?q=77000");
    assert_eq!(res.len(), 10);
    assert!(get_values(&res, "postcode")
        .iter()
        .all(|r| r.contains("77000"),));
    let types = get_types(&res);
    // since we did not ask for an house number, we should get none
    assert_eq!(count_types(&types, "house"), 0);
}

fn three_cities_zip_code_address_test(bragi: &mut BragiHandler) {
    let all_20 = bragi.get("/autocomplete?q=77288 2 Rue de la Reine Blanche");
    assert_eq!(all_20.len(), 1);
    assert!(get_values(&all_20, "postcode")
        .iter()
        .all(|r| *r == "77288",));
    let types = get_types(&all_20);
    let count = count_types(&types, "street");
    assert_eq!(count, 0);

    let count = count_types(&types, "city");
    assert_eq!(count, 0);

    let count = count_types(&types, "house");
    assert_eq!(count, 1);

    assert_eq!(
        get_values(&all_20, "label"),
        vec!["2 Rue de la Reine Blanche (Melun)"]
    );
}

fn three_cities_shape_test(bragi: &mut BragiHandler) {
    //      A ---------------------D
    //      |                      |
    //      |        === street    |
    //      |                      |
    //      |                      |
    //      B ---------------------C
    // Search with shape where street in shape
    let shape = r#"{"shape": {"type": "Feature","properties":{},"geometry":{"type":"Polygon",
        "coordinates": [[[2.656546, 48.537227],
        [2.657608, 48.537244],[2.657340, 48.536602],[2.656476, 48.536545],[2.656546, 48.537227]]]}}}"#;

    let geocodings = bragi.post("/autocomplete?q=Rue du Port&shape_scope[]=street", shape);
    assert_eq!(geocodings.len(), 1);
    assert_eq!(
        get_values(&geocodings, "label"),
        vec!["Rue du Port (Melun)"]
    );

    //      A ---------------------D
    //      |                      |
    //      |                      | === street
    //      |                      |
    //      |                      |
    //      B ---------------------C
    // Search with shape where street outside shape
    let shape = r#"{"shape": {"type": "Feature","properties":{},"geometry":{"type":"Polygon",
        "coordinates":[[[2.656546, 68.537227],
        [2.657608, 68.537244],[2.657340, 68.536602],[2.656476, 68.536545],[2.656546, 68.537227]]]}}}"#;

    let geocodings = bragi.post("/autocomplete?q=Rue du Port&shape_scope[]=street", shape);
    assert_eq!(geocodings.len(), 0);

    //      A ---------------------D
    //      |                      |
    //      |        X Melun       |
    //      |                      |
    //      |                      |
    //      B ---------------------C
    // Search with shape where admin in shape
    let shape = r#"{"shape": {"type": "Feature","properties":{},"geometry":{"type":"Polygon",
        "coordinates":[[[2.656546, 48.538927]
        ,[2.670816, 48.538927],[2.676476, 48.546545],[2.656546, 48.546545],[2.656546, 48.538927]]]}}}"#;

    let geocodings = bragi.post(
        "/autocomplete?q=Melun&shape_scope[]=admin&shape_scope[]=street",
        shape,
    );
    assert_eq!(geocodings.len(), 1);
    assert_eq!(get_values(&geocodings, "name"), vec!["Melun"]);

    //      A ---------------------D
    //      |                      |
    //      |                      | X Melun
    //      |                      |
    //      |                      |
    //      B ---------------------C
    // Search with shape where admin outside shape
    let shape = r#"{"shape": {"type": "Feature","properties":{},"geometry":{"type":"Polygon",
        "coordinates":[[[2.656546, 66.538927],
        [2.670816, 68.538927],[2.676476, 68.546545],[2.656546, 68.546545],[2.656546, 66.538927]]]}}}"#;

    let geocodings = bragi.post(
        "/autocomplete?q=Melun&shape_scope[]=admin&shape_scope[]=street",
        shape,
    );
    assert_eq!(geocodings.len(), 0);

    // shape filtering does not apply to stop areas.
    //
    //      A ---------------------D                 48.5372
    //      |                      |
    //      |      ===== street1   | ==== street2
    //      |                      |
    //      |      O stop1         | O stop2
    //      B ---------------------C                 48.5366
    //
    //      2.6565                 2.6576
    //
    // Search with shape and stops, we should have, based on the diagram:
    // - street1 visible     (because within shape)
    // - stop1 visible       (because within shape)
    // - street2 not visible (because outside of shape)
    // - stop2 visible       (because type=stop area)
    //
    // We also want to make sure street2 is visible if we don't use shape filtering

    let shape = r#"{"shape": {"type": "Feature","properties":{},"geometry":{"type":"Polygon",
        "coordinates": [[[2.6565, 48.5372],
        [2.6576, 48.5372],[2.6573, 48.5366],[2.6564, 48.5365],[2.6565, 48.5372]]]}}}"#;

    let geocodings = bragi.post("/autocomplete?q=Rue du Port&shape_scope[]=street", shape);
    assert_eq!(geocodings.len(), 1);
    assert_eq!(
        get_values(&geocodings, "label"),
        vec!["Rue du Port (Melun)"]
    );

    let geocodings = bragi.post("/autocomplete?q=Stop In&_all_data=true", shape);
    assert_eq!(geocodings.len(), 1);
    assert_eq!(get_values(&geocodings, "label"), vec!["Stop In (Melun)"]);

    let geocodings = bragi.post("/autocomplete?q=Stop Out&_all_data=true", shape);
    assert_eq!(geocodings.len(), 1);
    assert_eq!(get_values(&geocodings, "label"), vec!["Stop Out (Melun)"]);

    let geocodings = bragi.post("/autocomplete?q=Four&shape_scope[]=street", shape);
    assert_eq!(geocodings.len(), 0);

    let geocodings = bragi.get("/autocomplete?q=Four");
    assert_eq!(geocodings.len(), 1);
    assert_eq!(
        get_values(&geocodings, "label"),
        vec!["Rue du Four à Chaux (Livry-sur-Seine)"]
    );
}
