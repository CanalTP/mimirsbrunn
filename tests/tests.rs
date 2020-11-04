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
#![recursion_limit = "128"]

extern crate slog;
#[macro_use]
extern crate slog_scope;
#[macro_use]
extern crate approx;
#[macro_use]
extern crate assert_float_eq;

mod bano2mimir_test;
mod bragi_bano_test;
mod bragi_filter_types_test;
mod bragi_ntfs_test;
mod bragi_osm_test;
mod bragi_poi_test;
mod bragi_postcode_test;
mod bragi_stops_test;
mod bragi_synonyms_test;
mod bragi_three_cities_test;
mod canonical_import_process_test;
mod cosmogony2mimir_test;
mod openaddresses2mimir_test;
mod osm2mimir_bano2mimir_test;
mod osm2mimir_test;
mod poi2mimir_test;
mod rubber_test;
mod stops2mimir_test;

use docker_wrapper::*;
use serde_json::value::Value;
use serde_json::Map;
use std::process::Command;
use tools::*;

fn launch_and_assert(
    cmd: &str,
    args: &[std::string::String],
    es_wrapper: &ElasticSearchWrapper<'_>,
) {
    let status = Command::new(cmd).args(args).status().unwrap();
    let wale_instructions =
        "Run with DONT_KILL_THE_WHALE=true to keep elastic docker running for debugging";
    assert!(
        status.success(),
        "`{}` failed {}\n{}",
        cmd,
        &status,
        wale_instructions
    );
    es_wrapper.refresh();
}

pub fn get_values<'a>(r: &'a [Map<String, Value>], val: &'a str) -> Vec<&'a str> {
    r.iter().map(|e| get_value(e, val)).collect()
}

pub fn get_value<'a>(e: &'a Map<String, Value>, val: &str) -> &'a str {
    e.get(val).and_then(|l| l.as_str()).unwrap_or_else(|| "")
}

pub fn get_types(r: &[Map<String, Value>]) -> Vec<&str> {
    get_values(r, "type")
}

pub fn filter_by(r: &[Map<String, Value>], key: &str, t: &str) -> Vec<Map<String, Value>> {
    r.iter()
        .filter(|e| e.get(key).and_then(|l| l.as_str()).unwrap_or_else(|| "") == t)
        .cloned()
        .collect()
}

pub fn filter_by_type(r: &[Map<String, Value>], t: &str) -> Vec<Map<String, Value>> {
    filter_by(r, "type", t)
}

pub fn count_types(types: &[&str], value: &str) -> usize {
    types.iter().filter(|&t| *t == value).count()
}

fn get_poi_type_ids(e: &Map<String, Value>) -> Vec<&str> {
    let array = match e.get("poi_types").and_then(|json| json.as_array()) {
        None => return vec![],
        Some(array) => array,
    };
    array
        .iter()
        .filter_map(|v| v.as_object().and_then(|o| o.get("id")))
        .filter_map(|o| o.as_str())
        .collect()
}

fn get_first_index_aliases(indexes: &serde_json::Map<String, Value>) -> Vec<String> {
    indexes
        .get(indexes.keys().next().unwrap())
        .and_then(Value::as_object)
        .and_then(|s| s.get("aliases"))
        .and_then(Value::as_object)
        .map(|s| s.keys().cloned().collect())
        .unwrap_or_else(Vec::new)
}

/// Main test method (regroups all tests)
/// All tests are done sequentially,
/// and use the same docker in order to avoid multiple inits
/// (ES cleanup is handled by `es_wrapper`)
#[test]
fn all_tests() {
    let _guard = mimir::logger_init();
    let docker_wrapper = DockerWrapper::new().unwrap();

    // we call all tests here
    bano2mimir_test::bano2mimir_sample_test(ElasticSearchWrapper::new(&docker_wrapper));
    osm2mimir_test::osm2mimir_sample_test(ElasticSearchWrapper::new(&docker_wrapper));
    osm2mimir_test::osm2mimir_sample_test_sqlite(ElasticSearchWrapper::new(&docker_wrapper));
    stops2mimir_test::stops2mimir_sample_test(ElasticSearchWrapper::new(&docker_wrapper));
    osm2mimir_bano2mimir_test::osm2mimir_bano2mimir_test(ElasticSearchWrapper::new(
        &docker_wrapper,
    ));
    poi2mimir_test::poi2mimir_sample_test(ElasticSearchWrapper::new(&docker_wrapper));
    rubber_test::rubber_zero_downtime_test(ElasticSearchWrapper::new(&docker_wrapper));
    rubber_test::rubber_custom_id(ElasticSearchWrapper::new(&docker_wrapper));
    rubber_test::rubber_ghost_index_cleanup(ElasticSearchWrapper::new(&docker_wrapper));
    rubber_test::rubber_empty_bulk(ElasticSearchWrapper::new(&docker_wrapper));
    bragi_bano_test::bragi_bano_test(ElasticSearchWrapper::new(&docker_wrapper));
    bragi_osm_test::bragi_osm_test(ElasticSearchWrapper::new(&docker_wrapper));
    bragi_poi_test::test_i18n_poi(ElasticSearchWrapper::new(&docker_wrapper));
    bragi_three_cities_test::bragi_three_cities_test(ElasticSearchWrapper::new(&docker_wrapper));
    bragi_poi_test::bragi_poi_test(ElasticSearchWrapper::new(&docker_wrapper));
    bragi_poi_test::bragi_private_poi_test(ElasticSearchWrapper::new(&docker_wrapper));
    bragi_stops_test::bragi_stops_test(ElasticSearchWrapper::new(&docker_wrapper));
    bragi_ntfs_test::bragi_ntfs_test(ElasticSearchWrapper::new(&docker_wrapper));
    bragi_filter_types_test::bragi_filter_types_test(ElasticSearchWrapper::new(&docker_wrapper));
    bragi_synonyms_test::bragi_synonyms_test(ElasticSearchWrapper::new(&docker_wrapper));
    bragi_postcode_test::bragi_postcode_test(ElasticSearchWrapper::new(&docker_wrapper));
    openaddresses2mimir_test::oa2mimir_simple_test(ElasticSearchWrapper::new(&docker_wrapper));
    cosmogony2mimir_test::cosmogony2mimir_test(ElasticSearchWrapper::new(&docker_wrapper));
    canonical_import_process_test::canonical_import_process_test(ElasticSearchWrapper::new(
        &docker_wrapper,
    ));
    canonical_import_process_test::bragi_invalid_es_test(ElasticSearchWrapper::new(
        &docker_wrapper,
    ));
}
