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

use mimir;
use mimir::Members;
use std::path::Path;

/// Simple call to a BANO load into ES base
/// Checks that we are able to find one object (a specific address)
pub fn osm2mimir_sample_test(es_wrapper: crate::ElasticSearchWrapper<'_>) {
    let osm2mimir = Path::new(env!("OUT_DIR"))
        .join("../../../osm2mimir")
        .display()
        .to_string();
    crate::launch_and_assert(
        &osm2mimir,
        &[
            "--input=./tests/fixtures/osm_fixture.osm.pbf".into(),
            "--config-dir=./config".into(),
            "--import-way".into(),
            "--import-admin".into(),
            "--import-poi".into(),
            "--level=8".into(),
            "--level=7".into(),
            format!("--connection-string={}", es_wrapper.host()),
        ],
        &es_wrapper,
    );

    check_results(es_wrapper, "btreemap backend");
}

/// Simple call to a BANO load into ES base
/// Checks that we are able to find one object (a specific address)
pub fn osm2mimir_sample_test_sqlite(es_wrapper: crate::ElasticSearchWrapper<'_>) {
    let osm2mimir = Path::new(env!("OUT_DIR"))
        .join("../../../osm2mimir")
        .display()
        .to_string();
    crate::launch_and_assert(
        &osm2mimir,
        &[
            "--input=./tests/fixtures/osm_fixture.osm.pbf".into(),
            "--config-dir=./config".into(),
            "--import-way".into(),
            "--import-admin".into(),
            "--import-poi".into(),
            "--level=8".into(),
            "--level=7".into(),
            #[cfg(feature = "db-storage")]
            "--db-file=test-db.sqlite3".into(),
            "--db-buffer-size=1".into(),
            format!("--connection-string={}", es_wrapper.host()),
        ],
        &es_wrapper,
    );

    check_results(es_wrapper, "sqlite backend");
}

fn check_results(es_wrapper: crate::ElasticSearchWrapper<'_>, test_name: &str) {
    // Test: Import of Admin
    let res: Vec<_> = es_wrapper
        .search_and_filter("label:Livry-sur-Seine", |p| {
            // Eliminate other cities of the form ".*-sur-Seine"
            p.label().contains("Livry-sur-Seine")
        })
        .collect();
    assert_eq!(res.len(), 4, "{}", test_name);

    let has_boundary = |res: &Vec<mimir::Place>, is_admin: bool| {
        res.iter()
            .filter(|place| place.is_admin() == is_admin)
            .flat_map(|a| a.admins())
            .all(|a| a.boundary.clone().map_or(false, |b| !b.0.is_empty()))
    };
    // Admins have boundaries
    assert!(has_boundary(&res, true), "{}", test_name);

    // Others places than Admin don't
    assert!(!has_boundary(&res, false), "{}", test_name);

    assert!(res.iter().any(|r| r.is_admin()), "{}", test_name);

    // Test that Créteil (admin_level 7) is not treated as a city (level 8)
    let admin_regions: Vec<_> = es_wrapper
        .search_and_filter("label:Créteil", |_| true)
        .collect();
    assert!(!admin_regions.is_empty(), "{}", test_name);
    if let mimir::Place::Admin(ref creteil) = admin_regions[0] {
        assert_eq!(creteil.name, "Créteil", "{}", test_name);
        assert_eq!(creteil.level, 7, "{}", test_name);
        assert!(creteil.zone_type.is_none(), "{}", test_name);
    } else {
        panic!("creteil should be an admin");
    }

    // Test: search for "Rue des Près"
    let res: Vec<_> = es_wrapper
        .search_and_filter("label:Rue des Près", |_| true)
        .collect();
    assert!(!res.is_empty(), "{}", test_name);
    assert!(res[0].is_street(), "{}", test_name);
    // The first hit should be "Rue des Près"
    assert_eq!(
        res[0].label(),
        "Rue des Près (Livry-sur-Seine)",
        "{}",
        test_name
    );

    // And there should be only ONE "Rue des Près"
    assert_eq!(
        res.iter()
            .filter(|place| place.is_street() && place.label() == "Rue des Près (Livry-sur-Seine)")
            .count(),
        1,
        "{}",
        test_name
    );

    // Test: Search for "Rue du Four à Chaux" in "Livry-sur-Seine"
    let place_filter = |place: &mimir::Place| {
        place.is_street()
            && place.label() == "Rue du Four à Chaux (Livry-sur-Seine)"
            && place
                .admins()
                .first()
                .map(|admin| admin.label() == "Livry-sur-Seine (77000)")
                .unwrap_or(false)
    };
    // As we merge all ways with same name and of the same admin(level=city_level)
    // Here we have only one way
    let four_a_chaux_street: Vec<mimir::Place> = es_wrapper
        .search_and_filter("label:Rue du Four à Chaux (Livry-sur-Seine)", place_filter)
        .collect();

    let nb = four_a_chaux_street.len();
    assert_eq!(nb, 1, "{}", test_name);

    // Test the id is the min(=40812939) of all the ways composing the street
    assert!(
        four_a_chaux_street[0].address().map_or(false, |a| {
            if let mimir::Address::Street(s) = a {
                s.id == "street:osm:way:40812939"
            } else {
                false
            }
        }),
        "{}",
        test_name
    );

    // Test: Streets having the same label in different cities
    let place_filter = |place: &mimir::Place| {
        place.is_street()
            && place.label() == "Rue du Port (Melun)"
            && place
                .admins()
                .first()
                .map(|admin| admin.label() == "Melun (77000-CP77001)")
                .unwrap_or(false)
    };
    let nb = es_wrapper
        .search_and_filter("label:Rue du Port (Melun)", place_filter)
        .count();
    assert_eq!(nb, 1, "{}", test_name);

    // Test: Street admin is based on a middle node
    // (instead of the first node which is located outside Melun)
    let res: Vec<_> = es_wrapper
        .search_and_filter("label:Rue Marcel Houdet", |_| true)
        .collect();
    assert!(!res.len() != 0, "{}", test_name);
    assert_eq!(res[0].label(), "Rue Marcel Houdet (Melun)", "{}", test_name);
    assert!(res[0]
        .admins()
        .iter()
        .filter(|a| a.is_city())
        .any(|a| a.name == "Melun"));

    // Test: search Pois by label
    let res: Vec<_> = es_wrapper
        .search_and_filter("label:Le-Mée-sur-Seine Courtilleraies", |_| true)
        .collect();
    assert!(!res.is_empty(), "{}", test_name);

    let poi_type_post_office = "poi_type:amenity:post_office";
    assert!(
        res.iter().any(|r| r
            .poi()
            .map_or(false, |poi| poi.poi_type.id == poi_type_post_office)),
        "{}",
        test_name
    );

    let res: Vec<_> = es_wrapper
        .search_and_filter("label:Melun Rp", |_| true)
        .collect();
    assert!(!res.is_empty(), "{}", test_name);
    assert!(
        res.iter().any(|r| r
            .poi()
            .map_or(false, |poi| poi.poi_type.id == poi_type_post_office)),
        "{}",
        test_name
    );

    // Test: Certain kind of highways should not be indexed.
    // node id 451237947 is a 'bus_stop', and should not be found.
    let res: Vec<_> = es_wrapper
        .search_and_filter("label:Grand Châtelet", |_| true)
        .collect();
    assert!(res.is_empty(), "{}", test_name);

    // "Rue de Villiers" is at the exact neighborhood between two cities, a
    // document must be added for both.
    let res: Vec<_> = es_wrapper.search_and_filter("Villiers", |_| true).collect();
    assert_eq!(2, res.len());

    for city in &["Neuilly-sur-Seine", "Levallois-Perret"] {
        assert!(res
            .iter()
            .any(|poi| poi.admins().iter().any(|admin| &admin.name == city)));
    }
}
