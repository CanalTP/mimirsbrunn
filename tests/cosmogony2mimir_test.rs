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

use cosmogony::ZoneType;
use mimir;
use std::collections::BTreeMap;
use std::f64;
use std::path::Path;

/// load a cosmogony file in mimir.
/// The cosmogony file has been generated using the osm_fixture.osm.pbf file
pub fn cosmogony2mimir_test(es_wrapper: crate::ElasticSearchWrapper<'_>) {
    let cosmogony2mimir = Path::new(env!("OUT_DIR"))
        .join("../../../cosmogony2mimir")
        .display()
        .to_string();
    crate::launch_and_assert(
        &cosmogony2mimir,
        &[
            "--lang=fr".into(),
            "--lang=ru".into(),
            "--input=./tests/fixtures/cosmogony.json".into(),
            format!("--connection-string={}", es_wrapper.host()),
        ],
        &es_wrapper,
    );

    // we should be able to find the imported admins

    // All results should be admins, and have some basic information
    let all_objects: Vec<_> = es_wrapper.search_and_filter("label:*", |_| true).collect();
    assert_eq!(all_objects.len(), 7);

    assert!(all_objects.iter().any(|r| r.is_admin()));
    // all cosmogony admins have boundaries
    assert!(all_objects.iter().all(|r| r.admins()[0].boundary.is_some()));
    assert!(all_objects.iter().all(|r| r.admins()[0].coord.is_valid()));

    // check Livry-sur-Seine
    let res: Vec<_> = es_wrapper
        .search_and_filter("label:Livry-sur-Seine", |_| true)
        .collect();
    assert!(res.len() >= 1);

    let livry_sur_seine = &res[0];
    match livry_sur_seine {
        &mimir::Place::Admin(ref livry_sur_seine) => {
            assert_eq!(livry_sur_seine.id, "admin:osm:relation:215390");
            assert_eq!(livry_sur_seine.name, "Livry-sur-Seine");
            assert_eq!(
                livry_sur_seine.label,
                "Livry-sur-Seine (77000), Fausse Seine-et-Marne, France hexagonale"
            );
            assert_eq!(livry_sur_seine.insee, "77255");
            assert_eq!(livry_sur_seine.level, 8);
            assert_eq!(livry_sur_seine.zip_codes, vec!["77000"]);
            assert_relative_eq!(
                livry_sur_seine.weight,
                0.0000013707142857142856,
                epsilon = f64::EPSILON
            );
            assert!(livry_sur_seine.coord.is_valid());
            assert_eq!(livry_sur_seine.zone_type, Some(ZoneType::City));
        }
        _ => panic!("should be an admin"),
    }

    // check the state_district Fausse Seine-et-Marne
    let res: Vec<_> = es_wrapper
        .search_and_filter("name:Seine-et-Marne", |_| true)
        .collect();
    assert!(res.len() >= 1);

    let sem = &res[0];
    match sem {
        &mimir::Place::Admin(ref sem) => {
            assert_eq!(sem.name, "Fausse Seine-et-Marne");
            assert_eq!(sem.id, "admin:osm:relation:424253843");
            assert_eq!(sem.label, "Fausse Seine-et-Marne, France hexagonale");
            assert_eq!(sem.insee, "77");
            assert_eq!(sem.zip_codes, Vec::<String>::new());
            assert_eq!(sem.weight, 0f64);
            assert!(sem.coord.is_valid());
            assert_eq!(sem.zone_type, Some(ZoneType::StateDistrict));
            assert_eq!(sem.administrative_regions.len(), 1);
            assert_eq!(
                sem.administrative_regions[0].id,
                "admin:osm:relation:424256272"
            );
            assert_eq!(sem.administrative_regions[0].name, "France hexagonale");
        }
        _ => panic!("should be an admin"),
    }

    // we can even get the whole france
    let res: Vec<_> = es_wrapper
        .search_and_filter("name:France", |_| true)
        .collect();
    assert!(res.len() >= 1);

    let fr = &res[0];
    match fr {
        &mimir::Place::Admin(ref fr) => {
            assert_eq!(fr.id, "admin:osm:relation:424256272");
            assert_eq!(fr.name, "France hexagonale");
            assert_eq!(fr.label, "France hexagonale");
            assert_eq!(fr.insee, "");
            assert_eq!(fr.level, 2);
            assert_eq!(fr.zip_codes, Vec::<String>::new());
            assert_eq!(
                fr.codes
                    .iter()
                    .map(|c| (c.name.as_str(), c.value.as_str()))
                    .collect::<BTreeMap<_, _>>(),
                vec![
                    ("ISO3166-1", "FR"),
                    ("ISO3166-1:alpha2", "FR"),
                    ("ISO3166-1:alpha3", "FRA"),
                    ("ISO3166-1:numeric", "250"),
                    ("wikidata", "Q142"),
                ]
                .into_iter()
                .collect()
            );
            assert_relative_eq!(fr.weight, 0.04505024571428572, epsilon = f64::EPSILON);
            assert!(fr.coord.is_valid());
            assert_eq!(fr.zone_type, Some(ZoneType::Country));
            assert!(fr
                .names
                .0
                .iter()
                .any(|p| p.key == "ru" && p.value == "Метрополия Франции"));

            assert!(fr
                .labels
                .0
                .iter()
                .any(|p| p.key == "ru" && p.value == "Метрополия Франции"));
        }
        _ => panic!("should be an admin"),
    }

    // we check the weight is max on the admin with the highest population number
    let res: Vec<_> = es_wrapper
        .search_and_filter("label:Melun", |_| true)
        .collect();
    assert!(res.len() >= 1);

    let fausse_seine_max_weight = &res[0];
    match fausse_seine_max_weight {
        &mimir::Place::Admin(ref a) => {
            assert_eq!(a.id, "admin:osm:relation:80071");
            assert_relative_eq!(a.weight, 0.000028277857142857143, epsilon = f64::EPSILON);
        }
        _ => panic!("should be an admin"),
    }
}
