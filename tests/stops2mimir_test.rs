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

use std::path::Path;

/// Simple call to a stops2mimir load into ES base
/// Checks that we are able to find one object (a specific address)
pub fn stops2mimir_sample_test(es_wrapper: crate::ElasticSearchWrapper<'_>) {
    let stops2mimir = Path::new(env!("OUT_DIR"))
        .join("../../../stops2mimir")
        .display()
        .to_string();
    crate::launch_and_assert(
        &stops2mimir,
        &[
            "--input=./tests/fixtures/stops.txt".into(),
            format!("--connection-string={}", es_wrapper.host()),
            "--dataset=dataset1".into(),
        ],
        &es_wrapper,
    );
    // Test: Import of stops
    let res: Vec<_> = es_wrapper.search_and_filter("*", |_| true).collect();
    assert_eq!(res.len(), 6);
    assert!(res.iter().all(|r| r.is_stop()));

    // Test: search for stop area not in ES base
    let res: Vec<_> = es_wrapper
        .search_and_filter("label:unknown", |_| true)
        .collect();
    assert!(res.is_empty());

    // Test: search for "République"
    let res: Vec<_> = es_wrapper
        .search_and_filter("label:République", |_| true)
        .collect();
    assert!(res.len() == 1);
    assert_eq!(res[0].label(), "République");
    assert!(res[0].admins().is_empty());

    // we also test that all the stops have been imported in the global index
    let res: Vec<_> = es_wrapper
        .search_and_filter_on_global_stop_index("*", |_| true)
        .collect();
    assert_eq!(res.len(), 6);
    assert!(res.iter().all(|r| r.is_stop()));

    // we then import another stop fixture
    crate::launch_and_assert(
        &stops2mimir,
        &[
            "--input=./tests/fixtures/stops_dataset2.txt".into(),
            format!("--connection-string={}", es_wrapper.host()),
            "--dataset=dataset2".into(),
        ],
        &es_wrapper,
    );

    // we should now have 7 stops as there are 2 stops in dataset2,
    // but one (SA:known_by_all_dataset) is merged
    let res: Vec<_> = es_wrapper
        .search_and_filter_on_global_stop_index("*", |_| true)
        .collect();
    assert_eq!(res.len(), 7);
    for s in res {
        match s {
            mimir::Place::Stop(stop) => {
                match stop.id.as_ref() {
                    "stop_area:SA:known_by_all_dataset" => {
                        assert_eq!(stop.coverages, vec!["dataset1", "dataset2"]);
                        // we don't control which label is taken
                        assert!(vec!["All known stop", "All known stop, but different name"]
                            .contains(&stop.label.as_ref()));
                    }
                    "stop_area:SA:second_station:dataset2" => {
                        assert_eq!(stop.coverages, vec!["dataset2"])
                    }
                    _ => assert_eq!(stop.coverages, vec!["dataset1"]),
                }
            }
            _ => unreachable!(),
        }
    }
}
