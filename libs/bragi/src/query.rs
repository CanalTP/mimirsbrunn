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
use super::model;
use rs_es;
use rs_es::error::EsError;
use rs_es::query::Query as rs_q;
use rs_es::operations::search::SearchResult;
use rs_es::units as rs_u;
use mimir;
use serde_json;
use serde;
use mimir::objects::{MimirObject, Admin, Addr, Stop};

/// takes a ES json blob and build a Place from it
/// it uses the _type field of ES to know which type of the Place enum to fill
pub fn make_place(doc_type: String, value: Option<Box<serde_json::Value>>) -> Option<mimir::Place> {
    value.and_then(|v| {
        fn convert<T: serde::Deserialize>(v: serde_json::Value,
                                          f: fn(T) -> mimir::Place)
                                          -> Option<mimir::Place> {
            serde_json::from_value::<T>(v)
                .map_err(|err| warn!("Impossible to load ES result: {}", err))
                .ok()
                .map(f)
        }
        match doc_type.as_ref() {
            "addr" => convert(*v, mimir::Place::Addr),
            "street" => convert(*v, mimir::Place::Street),
            "admin" => convert(*v, mimir::Place::Admin),
            "poi" => convert(*v, mimir::Place::Poi),
            "stop" => convert(*v, mimir::Place::Stop),
            _ => {
                warn!("unknown ES return value, _type field = {}", doc_type);
                None
            }
        }
    })
}

#[derive(Debug, Eq, PartialEq)]
enum MatchType {
    Prefix,
    Fuzzy,
}

/// Create a `rs_es::Query` that boosts results according to the
/// distance to `coord`.
fn build_proximity(coord: &model::Coord) -> rs_q {
    rs_q::build_function_score()
        .with_boost_mode(rs_es::query::compound::BoostMode::Multiply)
        .with_boost(100000)
        .with_function(
            rs_es::query::functions::Function::build_decay(
                "coord",
                rs_u::Location::LatLon(coord.lat, coord.lon),
                rs_u::Distance::new(50f64, rs_u::DistanceUnit::Kilometer)
            ).build_gauss()
        ).build()
}

fn build_query(q: &str,
               match_type: MatchType,
               coord: &Option<model::Coord>,
               shape: Option<Vec<rs_es::units::Location>>)
               -> rs_es::query::Query {

    use rs_es::query::functions::Function;
    // we order the type of object we want
    // Note: the addresses are boosted more because even if we don't want them first
    // because they are more severely filtered
    let boost_addr = rs_q::build_term("_type", Addr::doc_type()).with_boost(5000).build();
    let boost_admin = rs_q::build_term("_type", Admin::doc_type()).with_boost(3000).build();
    let boost_stop = rs_q::build_term("_type", Stop::doc_type()).with_boost(2000).build();

    let main_match_type = match match_type {
        MatchType::Prefix => "label.prefix",
        MatchType::Fuzzy => "label.ngram",
    };

    let boost_main_match_query = rs_q::build_match(main_match_type.to_string(), q.to_string())
        .with_boost(500)
        .build();

    let boost_zipcode_match_query = rs_q::build_match("zip_codes.prefix".to_string(),
                                                      q.to_string())
        .with_boost(100)
        .build();

    let mut should_query = vec![boost_addr,
                                boost_admin,
                                boost_stop,
                                boost_main_match_query,
                                boost_zipcode_match_query];
    // for fuzzy search we also search by the prefix index (with a greater boost than ngram)
    // to have better results
    if match_type == MatchType::Fuzzy {
        should_query.push(rs_q::build_match("label.prefix", q.to_string())
            .with_boost(1000)
            .build());
    }

    if let &Some(ref c) = coord {
        // if we have coordinate, we boost we result near this coordinate
        should_query.push(build_proximity(c));
    } else {
        // if we don't have coords, we take the field `weight` into account
        let boost_on_weight = rs_q::build_function_score()
            .with_boost_mode(rs_es::query::compound::BoostMode::Multiply)
            .with_boost(500)
            .with_query(rs_q::build_match_all().build())
            .with_function(Function::build_field_value_factor("weight")
                .with_factor(1)
                .with_modifier(rs_es::query::functions::Modifier::Log1p)
                .build())
            .build();
        should_query.push(boost_on_weight);

        should_query.push(rs_q::build_match("administrative_regions.label".to_string(), q.to_string())
                 .with_boost(50)
                 .build());
        should_query.push(rs_q::build_match("street.administrative_regions.label".to_string(),
                                            q.to_string())
            .with_boost(50)
            .build())
    }

    let sub_query = rs_q::build_bool()
        .with_should(should_query)
        .build();

    // filter to handle house number
    // we either want:
    // * to exactly match the document house_number
    // * or that the document has no house_number
    let first_condition = rs_q::build_bool()
        .with_should(vec![rs_q::build_bool()
                              .with_must_not(rs_q::build_exists("house_number").build())
                              .build(),
                          rs_q::build_match("house_number", q.to_string()).build()])
        .build();

    use rs_es::query::MinimumShouldMatch;

    let second_condition = match match_type {
        // When the match type is Prefix, we want to use every possible information even though
        // these are not present in label, for instance, the zip_code. The cross_fields match type
        // allows to do the trick.
        // Ex:
        //   q : 20 rue hector malot 75012
        // WITHOUT the cross_fields match type, it will match neither "label" nor "zip_codes" and
        // the request will be treated by Fuzzy later, it's a pitty, because the adresse is actually
        // well spelt.
        // WITH the cross_fields match type, the request will be spilted into terms to match
        // "label" and "zip_codes"
        MatchType::Prefix => {
            rs_q::build_multi_match(vec!["label.prefix".to_string(),
                                         "zip_codes.prefix".to_string()],
                                    q.to_string())
                .with_type(rs_es::query::full_text::MatchQueryType::CrossFields)
                .with_operator("and")
                .build()
        }
        // for fuzzy search we lower our expectation & we accept 40% of token match on label.ngram
        // The "40%" is empirical, it's supposed to be able to manage cases BOTH missspelt one-word
        // requests AND very long requests.
        // Missspelt one-word request:
        //     Vaureaaal (instead of Vaureal)
        // Very long requests:
        //     Caisse Primaire d'Assurance Maladie de Haute Garonne, 33 Rue du Lot, 31100 Toulouse
        MatchType::Fuzzy => {
            rs_q::build_multi_match(vec!["label.ngram".to_string(), "zip_codes.prefix".to_string()],
                                    q.to_string())
                .with_minimum_should_match(MinimumShouldMatch::from(40f64))
                .build()
        }
    };

    let mut must = vec![first_condition, second_condition];

    if let Some(s) = shape {
        must.push(rs_q::build_geo_polygon("coord", s).build());
    }
    let filter = rs_q::build_bool()
        .with_must(must)
        .build();

    rs_q::build_bool()
        .with_must(vec![sub_query])
        .with_filter(filter)
        .build()
}

fn is_existing_index(client: &mut rs_es::Client, index: &str) -> Result<bool, EsError> {
    if index.is_empty() {
        return Ok(false);
    }
    match client.open_index(&index) {
        //This error indicates that the search index is absent in ElasticSearch.
        Err(EsError::EsError(_)) => Ok(false),
        Err(e) => Err(e),
        Ok(_) => Ok(true),
    }
}

fn get_indexes_by_type(a_type: &str) -> String {
    let doc_type = match a_type {
        "public_transport:stop_area" => "stop",
        "city" => "admin",
        "house" => "addr",
        _ => a_type,
    };

    format!("munin_{}", doc_type)
}

fn make_indexes_impl<F: FnMut(&str) -> Result<bool, EsError>>(all_data: bool,
                                                              pt_dataset_index: &Option<String>,
                                                              types: &Option<Vec<&str>>,
                                                              mut is_existing_index: F)
                                                              -> Result<Vec<String>, EsError> {
    if all_data {
        return Ok(vec!["munin".to_string()]);
    }

    let mut result: Vec<String> = vec![];
    let mut push = |result: &mut Vec<_>, i: &str| -> Result<(), EsError> {
        if try!(is_existing_index(i)) {
            result.push(i.into());
        }
        Ok(())
    };
    if let Some(ref types) = *types {
        for type_ in types.iter().filter(|t| **t != "public_transport:stop_area") {
            try!(push(&mut result, &get_indexes_by_type(type_)));
        }
        match *pt_dataset_index {
            Some(ref index) if types.contains(&"public_transport:stop_area") => {
                try!(push(&mut result, index));
            }
            _ => (),
        }
    } else {
        try!(push(&mut result, &"munin_geo_data".to_string()));

        if let Some(ref dataset) = *pt_dataset_index {
            try!(push(&mut result, &dataset.clone()));
        }
    }

    Ok(result)
}

fn make_indexes(all_data: bool,
                pt_dataset_index: &Option<String>,
                types: &Option<Vec<&str>>,
                client: &mut rs_es::Client)
                -> Result<Vec<String>, EsError> {
    make_indexes_impl(all_data,
                      pt_dataset_index,
                      types,
                      |index| is_existing_index(client, index))
}

fn collect(result: SearchResult<serde_json::Value>) -> Result<Vec<mimir::Place>, EsError> {
    debug!("{} documents found", result.hits.total);
    // for the moment rs-es does not handle enum Document,
    // so we need to convert the ES glob to a Place
    Ok(result.hits
        .hits
        .into_iter()
        .filter_map(|hit| make_place(hit.doc_type, hit.source))
        .collect())
}

fn query(q: &str,
         pt_dataset: &Option<&str>,
         all_data: bool,
         cnx: &str,
         match_type: MatchType,
         offset: u64,
         limit: u64,
         coord: &Option<model::Coord>,
         shape: Option<Vec<rs_es::units::Location>>,
         types: &Option<Vec<&str>>)
         -> Result<Vec<mimir::Place>, EsError> {
    let query = build_query(q, match_type, coord, shape);

    let mut client = rs_es::Client::new(cnx).unwrap();

    let pt_dataset_index = pt_dataset.map(|d| format!("munin_stop_{}", d));
    let indexes = try!(make_indexes(all_data, &pt_dataset_index, types, &mut client));

    debug!("ES indexes: {:?}", indexes);

    if indexes.is_empty() {
        // if there is no indexes, rs_es search with index "_all"
        // but we want to return empty response in this case.
        return Ok(vec![]);
    }

    let result: SearchResult<serde_json::Value> = try!(client.search_query()
        .with_indexes(&indexes.iter().map(|index| index.as_str()).collect::<Vec<&str>>())
        .with_query(&query)
        .with_from(offset)
        .with_size(limit)
        .send());

    collect(result)
}

pub fn features(pt_dataset: &Option<&str>,
                all_data: bool,
                cnx: &str,
                id: &str)
                -> Result<Vec<mimir::Place>, EsError> {

    let val = rs_es::units::JsonVal::String(id.into());
    let build_ids = rs_q::build_ids(vec![val]).build();

    let filter = rs_q::build_bool()
        .with_must(vec![build_ids])
        .build();
    let query = rs_q::build_bool().with_filter(filter).build();

    let mut client = rs_es::Client::new(cnx).unwrap();

    let pt_dataset_index = pt_dataset.map(|d| format!("munin_stop_{}", d));
    let indexes = try!(make_indexes(all_data, &pt_dataset_index, &None, &mut client));

    debug!("ES indexes: {:?}", indexes);

    if indexes.is_empty() {
        // if there is no indexes, rs_es search with index "_all"
        // but we want to return an error in this case.
        return Err(EsError::EsError("Unable to find object".to_string()));
    }

    let result: SearchResult<serde_json::Value> = try!(client.search_query()
        .with_indexes(&indexes.iter().map(|index| index.as_str()).collect::<Vec<&str>>())
        .with_query(&query)
        .send());

    if result.hits.total == 0 {
        Err(EsError::EsError("Unable to find object".to_string()))
    } else {
        collect(result)
    }
}

/// Reverse geocoding request, that returns the house or street the
/// closest to the given `coord`.
pub fn reverse(coord: &model::Coord, cnx: &str) -> Result<Vec<mimir::Place>, EsError> {
    let mut client = rs_es::Client::new(cnx).unwrap();
    let types = vec!["house".into(), "street".into()];
    let indexes = make_indexes(false, &None, &Some(types), &mut client)?;
    let distance = rs_u::Distance::new(500., rs_u::DistanceUnit::Meter);
    let geo_distance = rs_q::build_geo_distance("coord", (coord.lat, coord.lon), distance).build();
    let query = rs_q::build_bool()
        .with_should(build_proximity(coord))
        .with_must(geo_distance)
        .build();
    let result: SearchResult<serde_json::Value> = client.search_query()
        .with_indexes(&indexes.iter().map(|index| index.as_str()).collect::<Vec<_>>())
        .with_query(&query)
        .with_size(1)
        .send()?;
    collect(result)
}

pub fn autocomplete(q: &str,
                    pt_dataset: &Option<&str>,
                    all_data: bool,
                    offset: u64,
                    limit: u64,
                    coord: Option<model::Coord>,
                    cnx: &str,
                    shape: Option<Vec<(f64, f64)>>,
                    types: Option<Vec<&str>>)
                    -> Result<Vec<mimir::Place>, EsError> {
    fn make_shape(shape: &Option<Vec<(f64, f64)>>) -> Option<Vec<rs_es::units::Location>> {
        shape.as_ref().map(|v| v.iter().map(|&l| l.into()).collect())
    }

    // First we try a prety exact match on the prefix.
    // If there are no results then we do a new fuzzy search (matching ngrams)
    let results = try!(query(&q,
                             &pt_dataset,
                             all_data,
                             cnx,
                             MatchType::Prefix,
                             offset,
                             limit,
                             &coord,
                             make_shape(&shape),
                             &types));
    if results.is_empty() {
        query(&q,
              &pt_dataset,
              all_data,
              cnx,
              MatchType::Fuzzy,
              offset,
              limit,
              &coord,
              make_shape(&shape),
              &types)
    } else {
        Ok(results)
    }
}

#[test]
fn test_make_indexes_impl() {
    fn ok_index(_index: &str) -> Result<bool, EsError> {
        Ok(true)
    }
    // all_data
    assert_eq!(make_indexes_impl(true, &None, &None, ok_index).unwrap(),
               vec!["munin"]);

    // no dataset and no types
    assert_eq!(make_indexes_impl(false, &None, &None, ok_index).unwrap(),
               vec!["munin_geo_data"]);

    // dataset fr + no types
    assert_eq!(make_indexes_impl(false, &Some("munin_stop_fr".to_string()), &None, ok_index)
                   .unwrap(),
               vec!["munin_geo_data", "munin_stop_fr"]);

    // no dataset + types poi, city, street, house and public_transport:stop_area
    // => munin_stop is not included
    assert_eq!(make_indexes_impl(false,
                                 &None,
                                 &Some(vec!["poi",
                                            "city",
                                            "street",
                                            "house",
                                            "public_transport:stop_area"]),
                                 ok_index)
                   .unwrap(),
               vec!["munin_poi", "munin_admin", "munin_street", "munin_addr"]);

    // no dataset fr + type public_transport:stop_area only
    assert_eq!(make_indexes_impl(false,
                                 &None,
                                 &Some(vec!["public_transport:stop_area"]),
                                 ok_index)
                   .unwrap(),
               Vec::<String>::new());

    // dataset fr + types poi, city, street, house and public_transport:stop_area
    assert_eq!(make_indexes_impl(false,
                                 &Some("munin_stop_fr".to_string()),
                                 &Some(vec!["poi",
                                            "city",
                                            "street",
                                            "house",
                                            "public_transport:stop_area"]),
                                 ok_index)
                   .unwrap(),
               vec!["munin_poi", "munin_admin", "munin_street", "munin_addr", "munin_stop_fr"]);

    // dataset fr types poi, city, street, house without public_transport:stop_area
    //  => munin_stop_fr is not included
    assert_eq!(make_indexes_impl(false,
                                 &Some("munin_stop_fr".to_string()),
                                 &Some(vec!["poi", "city", "street", "house"]),
                                 ok_index)
                   .unwrap(),
               vec!["munin_poi", "munin_admin", "munin_street", "munin_addr"]);

    // dataset fr types poi, city, street, house without public_transport:stop_area
    // and the function is_existing_index with a result "false" as non of the index
    // is present in elasticsearch
    assert_eq!(make_indexes_impl(false,
                                 &Some("munin_stop_fr".to_string()),
                                 &Some(vec!["poi", "city", "street", "house"]),
                                 |_index| Ok::<_, EsError>(false))
                   .unwrap(),
               Vec::<String>::new());

    // dataset fr types poi, city, street, house without public_transport:stop_area
    // and the function is_existing_index with an error in the result (Elasticsearch is absent..)
    match make_indexes_impl(false,
                            &Some("munin_stop_fr".to_string()),
                            &Some(vec!["poi", "city", "street", "house"]),
                            |_index| Err::<bool, _>(EsError::EsError("Elasticsearch".into()))) {
        Err(EsError::EsError(e)) => assert_eq!(e, "Elasticsearch"),
        _ => assert!(false),
    }
}
