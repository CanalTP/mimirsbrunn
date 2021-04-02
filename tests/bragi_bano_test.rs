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

use super::get_values;
use super::BragiHandler;
use serde_json::json;
use std::path::Path;

pub fn bragi_bano_test(es_wrapper: crate::ElasticSearchWrapper<'_>) {
    let mut bragi = BragiHandler::new(es_wrapper.host());

    // *********************************
    // We load bano files
    // *********************************
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

    status_test(&mut bragi);
    simple_bano_autocomplete_test(&mut bragi);
    simple_bano_shape_filter_test(&mut bragi);
    simple_bano_lon_lat_test(&mut bragi);
    long_bano_address_test(&mut bragi);
    reverse_bano_test(&mut bragi);
}

fn status_test(bragi: &mut BragiHandler) {
    assert_eq!(
        bragi.get_json("/status").pointer("/status"),
        Some(&json!("good"))
    );
}

fn simple_bano_autocomplete_test(bragi: &mut BragiHandler) {
    assert_eq!(
        bragi.get_json("/autocomplete?q=15 Rue Hector Malot (Paris)"),
        json!(
        {
            "features": [
                {
                    "geometry": {
                        "coordinates": [
                            2.376_379,
                            48.846_495
                        ],
                        "type": "Point"
                    },
                    "properties": {
                        "geocoding": {
                            "administrative_regions": [],
                            "city": null,
                            "citycode": null,
                            "country_codes": ["fr"],
                            "housenumber": "15",
                            "id": "addr:2.376379;48.846495:15",
                            "label": "15 Rue Hector Malot (Paris)",
                            "name": "15 Rue Hector Malot",
                            "postcode": "75012",
                            "street": "Rue Hector Malot",
                            "type": "house"
                        }
                    },
                    "type": "Feature"
                }
            ],
            "geocoding": {
                "query": "",
                "version": "0.1.0"
            },
            "type": "FeatureCollection"
        }
        )
    );
}

// A(48.846431 2.376488)
// B(48.846430 2.376306)
// C(48.846606 2.376309)
// D(48.846603 2.376486)
// R(48.846495 2.376378) : 15 Rue Hector Malot, (Paris)
// E(48.846452 2.376580) : 18 Rue Hector Malot, (Paris)
//
//             E
//
//      A ---------------------D
//      |                      |
//      |         R            |
//      |                      |
//      |                      |
//      B ---------------------C
fn simple_bano_shape_filter_test(bragi: &mut BragiHandler) {
    // Search with shape where house number in shape
    // We don't use a scope_shape, so it should be as if we didn't use any shape.
    let shape = r#"{"shape":{"type":"Feature","properties":{},"geometry":{"type":"Polygon",
        "coordinates":[[[2.376488, 48.846431],
        [2.376306, 48.846430],[2.376309, 48.846606],[2.376486, 48.846603], [2.376488, 48.846431]]]}}}"#;
    let r = bragi.post_as_json("/autocomplete?q=15 Rue Hector Malot, (Paris)", shape);
    assert_eq!(
        r,
        json!({
          "type": "FeatureCollection",
          "geocoding": {
            "version": "0.1.0",
            "query": ""
          },
          "features": [
            {
              "type": "Feature",
              "geometry": {
                "coordinates": [
                  2.376_379,
                  48.846_495
                ],
                "type": "Point"
              },
              "properties": {
                "geocoding": {
                  "id": "addr:2.376379;48.846495:15",
                  "type": "house",
                  "label": "15 Rue Hector Malot (Paris)",
                  "name": "15 Rue Hector Malot",
                  "housenumber": "15",
                  "street": "Rue Hector Malot",
                  "postcode": "75012",
                  "city": null,
                  "citycode": null,
                  "country_codes": ["fr"],
                  "administrative_regions": []
                }
              }
            }
          ]
        }
        )
    );

    // Search with shape where house number out of shape and house not in scope
    // since the house is not in the filter scope, it should be found
    let r = bragi.post_as_json(
        "/autocomplete?q=18 Rue Hector Malot, (Paris)&shape_scope[]=admin",
        shape,
    );
    assert_eq!(
        r,
        json!({
          "type": "FeatureCollection",
          "geocoding": {
            "version": "0.1.0",
            "query": ""
          },
          "features": [
            {
              "type": "Feature",
              "geometry": {
                "coordinates": [
                  2.376_58,
                  48.846_452
                ],
                "type": "Point"
              },
              "properties": {
                "geocoding": {
                  "id": "addr:2.37658;48.846452:18",
                  "type": "house",
                  "label": "18 Rue Hector Malot (Paris)",
                  "name": "18 Rue Hector Malot",
                  "housenumber": "18",
                  "street": "Rue Hector Malot",
                  "postcode": "75012",
                  "city": null,
                  "citycode": null,
                  "country_codes": ["fr"],
                  "administrative_regions": []
                }
              }
            }
          ]
        }
        )
    );
    // Search with shape where house number out of shape and house not in scope
    // since the house is in the filter scope, it should be filtered out.
    let r = bragi.post_as_json(
        "/autocomplete?q=18 Rue Hector Malot, (Paris)&shape_scope[]=addr",
        shape,
    );
    assert_eq!(
        r,
        json!({
          "type": "FeatureCollection",
          "geocoding": {
            "version": "0.1.0",
            "query": ""
          },
          "features": []
        }
        )
    );
}

fn simple_bano_lon_lat_test(bragi: &mut BragiHandler) {
    // test with a lon/lat priorisation
    // in the dataset there are two '20 rue hector malot',
    // one in paris and one in trifouilli-les-Oies
    // in the mean time we test our prefix search_query
    let all_20 = bragi.get("/autocomplete?q=20 rue hect mal");
    assert_eq!(all_20.len(), 2);
    // the first one is paris (since Paris has more streets, it is prioritized first)
    // TODO uncomment this test, for the moment since osm is not loaded, the order is random
    // assert_eq!(get_labels(&all_20),
    //            vec!["20 Rue Hector Malot (Paris)", "20 Rue Hector Malot (Trifouilli-les-Oies)"]);

    // if we give a lon/lat near trifouilli-les-Oies, we'll have another sort
    let all_20 = bragi.get("/autocomplete?q=20 rue hector malot&lat=50.2&lon=2.0");
    assert_eq!(
        get_values(&all_20, "label"),
        vec![
            "20 Rue Hector Malot (Trifouilli-les-Oies)",
            "20 Rue Hector Malot (Paris)",
        ]
    );
    // and when we're in paris, we get paris first
    let all_20 = bragi.get("/autocomplete?q=20 rue hector malot&lat=48&lon=2.4");
    assert_eq!(
        get_values(&all_20, "label"),
        vec![
            "20 Rue Hector Malot (Paris)",
            "20 Rue Hector Malot (Trifouilli-les-Oies)",
        ]
    );
}

fn long_bano_address_test(bragi: &mut BragiHandler) {
    // test with a very long request which consists of an exact address and something else
    // and the "something else" should not disturb the research
    let all_20 = bragi.get(
        "/autocomplete?q=The Marvellous Navitia Developers Kisio Digital 20 rue hector \
         malot paris",
    );
    assert_eq!(all_20.len(), 1);
    assert_eq!(
        get_values(&all_20, "label"),
        vec!["20 Rue Hector Malot (Paris)"]
    );
}

fn reverse_bano_test(bragi: &mut BragiHandler) {
    let res = bragi.get("/reverse?lon=2.37716&lat=48.8468");
    assert_eq!(res.len(), 1);
    assert_eq!(
        get_values(&res, "label"),
        vec!["20 Rue Hector Malot (Paris)"]
    );

    let res = bragi.get("/reverse?lon=1.3787628&lat=43.6681995");
    assert_eq!(
        get_values(&res, "label"),
        vec!["2 Rue des Pins (Beauzelle)"]
    );
}
