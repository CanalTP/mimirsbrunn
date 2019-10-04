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
use super::model::{self, BragiError};
use geojson::Geometry;
use mimir;
use mimir::objects::{Addr, Admin, Coord, MimirObject, Poi, Stop, Street};
use mimir::rubber::{get_indexes, read_places, Rubber};
use prometheus::{self, exponential_buckets, histogram_opts, register_histogram_vec, HistogramVec};
use rs_es;
use rs_es::error::EsError;
use rs_es::operations::search::Source;
use rs_es::query::compound::BoostMode;
use rs_es::query::functions::{DecayOptions, FilteredFunction, Function, Modifier};
use rs_es::query::Query;
use rs_es::units as rs_u;
use serde;
use serde_json;
use slog_scope::{debug, error, warn};
use std::{fmt, iter};

lazy_static::lazy_static! {
    static ref ES_REQ_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "bragi_elasticsearch_request_duration_seconds",
        "The elasticsearch request latencies in seconds.",
        &["search_type"],
        exponential_buckets(0.001, 1.5, 25).unwrap()
    )
    .unwrap();
}

/// takes a ES json blob and build a Place from it
/// it uses the _type field of ES to know which type of the Place enum to fill
pub fn make_place(doc_type: String, value: Option<Box<serde_json::Value>>) -> Option<mimir::Place> {
    value.and_then(|v| {
        fn convert<T>(v: serde_json::Value, f: fn(T) -> mimir::Place) -> Option<mimir::Place>
        where
            for<'de> T: serde::Deserialize<'de>,
        {
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

impl fmt::Display for MatchType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let printable = match *self {
            MatchType::Prefix => "prefix",
            MatchType::Fuzzy => "fuzzy",
        };
        write!(f, "{}", printable)
    }
}

// filter to handle PT coverages
// we either want:
// * to get objects with no coverage at all (non-PT objects)
// * or the objects with coverage matching the ones we're allowed to get
fn build_coverage_condition(pt_datasets: &[&str]) -> Query {
    Query::build_bool()
        .with_should(vec![
            Query::build_bool()
                .with_must_not(Query::build_exists("coverages").build())
                .build(),
            Query::build_terms("coverages")
                .with_values(pt_datasets)
                .build(),
        ])
        .build()
}

/// Create a `rs_es::Query` that boosts results according to the
/// distance to `coord`.
fn build_proximity_with_boost(coord: &Coord, weight: f64) -> Query {
    Query::build_function_score()
        .with_functions(vec![
            FilteredFunction::build_filtered_function(
                None,
                DecayOptions::new(
                    rs_u::Location::LatLon(coord.lat(), coord.lon()),
                    rs_u::Distance::new(130f64, rs_u::DistanceUnit::Kilometer),
                )
                .with_offset(rs_u::Distance::new(20f64, rs_u::DistanceUnit::Kilometer))
                .with_decay(0.4f64)
                .build("coord")
                .build_exp(),
                None,
            ),
            FilteredFunction::build_filtered_function(
                None,
                Function::build_weight(weight).build(),
                None,
            ),
        ])
        .with_boost_mode(BoostMode::Replace)
        .build()
}

fn build_with_weight<A: Into<Option<f64>>>(factor: A) -> Query {
    let factor = factor.into();
    let weighted = |doc_type, weight| {
        FilteredFunction::build_filtered_function(
            Query::build_term("_type", doc_type).build(),
            Function::build_field_value_factor("weight")
                .with_factor(factor.unwrap_or(0.15) * weight)
                .with_missing(0.0)
                .build(),
            None,
        )
    };

    Query::build_function_score()
        .with_functions(vec![
            weighted(Stop::doc_type(), 10.0),
            weighted(Addr::doc_type(), 1.0),
            weighted(Admin::doc_type(), 1.0),
            weighted(Poi::doc_type(), 1.0),
            weighted(Street::doc_type(), 1.0),
        ])
        .with_boost_mode(BoostMode::Replace)
        .build()
}

fn build_query<'a>(
    q: &str,
    match_type: MatchType,
    coord: Option<Coord>,
    shape: Option<Geometry>,
    pt_datasets: &[&str],
    all_data: bool,
    langs: &'a [&'a str],
    zone_types: &[&str],
    poi_types: &[&str],
) -> Query {
    // Priorization by type
    fn match_type_with_boost<T: MimirObject>(boost: f64) -> Query {
        Query::build_term("_type", T::doc_type())
            .with_boost(boost)
            .build()
    }
    let type_query = Query::build_bool()
        .with_should(vec![
            match_type_with_boost::<Addr>(30.),
            match_type_with_boost::<Admin>(19.),
            match_type_with_boost::<Stop>(18.),
            match_type_with_boost::<Poi>(1.5),
            match_type_with_boost::<Street>(1.),
        ])
        .with_boost(30.)
        .build();

    let format_names_field = |lang| format!("names.{}", lang);
    let format_labels_field = |lang| format!("labels.{}", lang);
    let format_labels_prefix_field = |lang| format!("labels.{}.prefix", lang);

    let build_multi_match =
        |default_field: &str, lang_field_formatter: &dyn Fn(&'a &'a str) -> String| {
            let boosted_i18n_fields = langs.iter().map(lang_field_formatter);
            let fields: Vec<String> = iter::once(default_field.into())
                .chain(boosted_i18n_fields)
                .collect();
            Query::build_multi_match(fields, q)
        };

    // Priorization by query string
    let mut string_should = vec![
        build_multi_match("name", &format_names_field)
            .with_boost(1.8)
            .build(),
        build_multi_match("label", &format_labels_field)
            .with_boost(0.6)
            .build(),
        build_multi_match("label.prefix", &format_labels_prefix_field)
            .with_boost(0.6)
            .build(),
        Query::build_match("zip_codes", q).with_boost(1.).build(),
        Query::build_match("house_number", q)
            .with_boost(0.001)
            .build(),
    ];
    if let MatchType::Fuzzy = match_type {
        let format_labels_ngram_field = |lang| format!("labels.{}.ngram", lang);
        string_should.push(if coord.is_some() {
            build_multi_match("label.ngram", &format_labels_ngram_field)
                .with_boost(3.8)
                .build()
        } else {
            build_multi_match("label.ngram", &format_labels_ngram_field)
                .with_boost(1.8)
                .build()
        });
    }
    let string_query = Query::build_bool()
        .with_should(string_should)
        .with_boost(1.)
        .build();

    let mut admin_weight = 0.03;

    // Priorization by importance
    let mut importance_queries = if let Some(ref coord) = coord {
        if let MatchType::Fuzzy = match_type {
            vec![
                build_with_weight(0.15),
                build_proximity_with_boost(coord, 0.4),
            ]
        } else {
            admin_weight = 0.12;
            vec![
                build_with_weight(0.4),
                build_proximity_with_boost(coord, 0.4),
            ]
        }
    } else {
        vec![build_with_weight(None)]
    };

    match match_type {
        MatchType::Prefix => {
            let admin_importance_query = Query::build_function_score()
                .with_query(Query::build_term("_type", Admin::doc_type()).build())
                .with_functions(vec![
                    FilteredFunction::build_filtered_function(
                        None,
                        Function::build_field_value_factor("weight")
                            .with_factor(1e6)
                            .with_modifier(Modifier::Log1p)
                            .with_missing(0.)
                            .build(),
                        None,
                    ),
                    FilteredFunction::build_filtered_function(
                        None,
                        Function::build_weight(admin_weight).build(),
                        None,
                    ),
                ])
                .with_boost_mode(BoostMode::Replace)
                .build();
            importance_queries.push(admin_importance_query);
        }
        MatchType::Fuzzy => {}
    };

    // filter to handle house number
    // we either want:
    // * to exactly match the document house_number
    // * or that the document has no house_number
    let house_number_condition = Query::build_bool()
        .with_should(vec![
            Query::build_bool()
                .with_must_not(Query::build_exists("house_number").build())
                .build(),
            Query::build_match("house_number", q.to_string()).build(),
        ])
        .build();

    use rs_es::query::CombinationMinimumShouldMatch;
    use rs_es::query::MinimumShouldMatch;

    let matching_condition = match match_type {
        // When the match type is Prefix, we want to use every possible information even though
        // these are not present in label, for instance, the zip_code.
        // The field full_label contains all of them and will do the trick.
        MatchType::Prefix => Query::build_match("full_label.prefix".to_string(), q.to_string())
            .with_operator("and")
            .build(),
        // for fuzzy search we lower our expectation & we accept a certain percentage of token match
        // on full_label.ngram
        // The values defined here are empirical,
        // it's supposed to be able to manage cases BOTH missspelt one-word
        // www.elastic.co/guide/en/elasticsearch/guide/current/match-multi-word.html#match-precision
        // requests AND very long requests.
        // Missspelt one-word request:
        //     Vaureaaal (instead of Vaureal)
        // Very long requests:
        //     Caisse Primaire d'Assurance Maladie de Haute Garonne, 33 Rue du Lot, 31100 Toulouse
        MatchType::Fuzzy => Query::build_match("full_label.ngram".to_string(), q.to_string())
            .with_minimum_should_match(MinimumShouldMatch::from(vec![
                CombinationMinimumShouldMatch::new(1i64, -1i64),
                CombinationMinimumShouldMatch::new(3i64, -2i64),
                CombinationMinimumShouldMatch::new(9i64, -4i64),
                CombinationMinimumShouldMatch::new(20i64, 25f64),
            ]))
            .build(),
    };

    let mut filters = vec![house_number_condition, matching_condition];

    // if searching through all data, no coverage filter
    if !all_data {
        filters.push(build_coverage_condition(pt_datasets));
    }

    // We want to limit the search to the geographic shape given in argument,
    // except for stop areas
    if let Some(s) = shape {
        let filter_wo_stop = Query::build_bool()
            .with_must(vec![
                Query::build_bool()
                    .with_must_not(Query::build_term("_type", Stop::doc_type()).build())
                    .build(),
                Query::build_geo_shape("approx_coord")
                    .with_geojson(s)
                    .build(),
            ])
            .build();
        let filter_w_stop = Query::build_term("_type", Stop::doc_type()).build();
        let geo_filter = Query::build_bool()
            .with_should(vec![filter_w_stop, filter_wo_stop])
            .build();
        filters.push(geo_filter);
    }

    let mut query = Query::build_bool()
        .with_must(vec![type_query, string_query])
        .with_should(importance_queries)
        .with_filter(Query::build_bool().with_must(filters).build());

    if !zone_types.is_empty() {
        query = query.with_filter(
            Query::build_bool()
                .with_should(
                    zone_types
                        .iter()
                        .map(|x| Query::build_match("zone_type", *x).build())
                        .collect::<Vec<_>>(),
                )
                .build(),
        );
    }
    if !poi_types.is_empty() {
        query = query.with_filter(
            Query::build_bool()
                .with_should(
                    poi_types
                        .iter()
                        .map(|x| Query::build_match("poi_type.id", *x).build())
                        .collect::<Vec<_>>(),
                )
                .build(),
        );
    }

    query.build()
}

fn query(
    q: &str,
    pt_datasets: &[&str],
    poi_datasets: &[&str],
    all_data: bool,
    rubber: &mut Rubber,
    match_type: MatchType,
    offset: u64,
    limit: u64,
    coord: Option<Coord>,
    shape: Option<Geometry>,
    types: &[&str],
    zone_types: &[&str],
    poi_types: &[&str],
    langs: &[&str],
    debug: bool,
) -> Result<Vec<mimir::Place>, EsError> {
    let query_type = match_type.to_string();
    let query = build_query(
        q,
        match_type,
        coord,
        shape,
        pt_datasets,
        all_data,
        langs,
        zone_types,
        poi_types,
    );

    let indexes = get_indexes(all_data, &pt_datasets, &poi_datasets, types);
    let indexes = indexes
        .iter()
        .map(|index| index.as_str())
        .collect::<Vec<&str>>();
    debug!("ES indexes: {:?}", indexes);

    if indexes.is_empty() {
        // if there is no indexes, rs_es search with index "_all"
        // but we want to return empty response in this case.
        return Ok(vec![]);
    }
    let timer = ES_REQ_HISTOGRAM
        .get_metric_with_label_values(&[query_type.as_str()])
        .map(|h| h.start_timer())
        .map_err(
            |err| error!("impossible to get ES_REQ_HISTOGRAM metrics"; "err" => err.to_string()),
        )
        .ok();

    let timeout = rubber.timeout.map(|t| format!("{:?}", t));
    let mut search_query = rubber.es_client.search_query();

    let search_query = search_query
        .with_ignore_unavailable(true)
        .with_indexes(&indexes)
        .with_query(&query)
        .with_from(offset)
        .with_size(limit)
        // No need to fetch "boundary" as it's not used in the geocoding response
        // and is very large in some documents (countries...)
        .with_source(Source::exclude(&["boundary"]));

    // We don't want to clutter the Query URL, so we only add an explanation if the option is used
    let search_query = match debug {
        true => search_query.with_explain(true),
        false => search_query,
    };

    if let Some(timeout) = &timeout {
        search_query.with_timeout(timeout.as_str());
    }
    let result = search_query.send()?;

    timer.map(|t| t.observe_duration());

    read_places(result, coord.as_ref())
}

pub fn features(
    pt_datasets: &[&str],
    poi_datasets: &[&str],
    all_data: bool,
    id: &str,
    mut rubber: Rubber,
) -> Result<Vec<mimir::Place>, BragiError> {
    let val = rs_es::units::JsonVal::String(id.into());
    let mut filters = vec![Query::build_ids(vec![val]).build()];

    // if searching through all data, no coverage filter
    if !all_data {
        filters.push(build_coverage_condition(pt_datasets));
    }
    let filter = Query::build_bool().with_must(filters).build();
    let query = Query::build_bool().with_filter(filter).build();

    let indexes = get_indexes(all_data, &pt_datasets, &poi_datasets, &[]);
    let indexes = indexes
        .iter()
        .map(|index| index.as_str())
        .collect::<Vec<&str>>();

    debug!("ES indexes: {:?}", indexes);

    if indexes.is_empty() {
        // if there is no indexes, rs_es search with index "_all"
        // but we want to return an error in this case.
        return Err(BragiError::ObjectNotFound);
    }

    let timer = ES_REQ_HISTOGRAM
        .get_metric_with_label_values(&["features"])
        .map(|h| h.start_timer())
        .map_err(
            |err| error!("impossible to get ES_REQ_HISTOGRAM metrics"; "err" => err.to_string()),
        )
        .ok();

    let timeout = rubber.timeout.map(|t| format!("{:?}", t));
    let mut search_query = rubber.es_client.search_query();

    let search_query = search_query
        .with_ignore_unavailable(true)
        .with_indexes(&indexes)
        .with_query(&query);

    if let Some(timeout) = &timeout {
        search_query.with_timeout(timeout.as_str());
    }

    let result = search_query.send()?;

    timer.map(|t| t.observe_duration());

    if result.hits.total == 0 {
        Err(BragiError::ObjectNotFound)
    } else {
        read_places(result, None).map_err(model::BragiError::from)
    }
}

pub fn autocomplete(
    q: &str,
    pt_datasets: &[&str],
    poi_datasets: &[&str],
    all_data: bool,
    offset: u64,
    limit: u64,
    coord: Option<Coord>,
    shape: Option<Geometry>,
    types: &[&str],
    zone_types: &[&str],
    poi_types: &[&str],
    langs: &[&str],
    mut rubber: Rubber,
    debug: bool,
) -> Result<Vec<mimir::Place>, BragiError> {
    // Perform parameters validation.
    if !zone_types.is_empty() && !types.iter().any(|s| *s == "zone") {
        return Err(BragiError::InvalidParam(
            "zone_type[] parameter requires to have 'type[]=zone'",
        ));
    }
    if !poi_types.is_empty() && !types.iter().any(|s| *s == "poi") {
        return Err(BragiError::InvalidParam(
            "poi_type[] parameter requires to have 'type[]=poi'",
        ));
    }

    // First we try a pretty exact match on the prefix.
    // If there are no results then we do a new fuzzy search (matching ngrams)
    let results = query(
        &q,
        &pt_datasets,
        &poi_datasets,
        all_data,
        &mut rubber,
        MatchType::Prefix,
        offset,
        limit,
        coord,
        shape.clone(),
        &types,
        &zone_types,
        &poi_types,
        &langs,
        debug,
    )
    .map_err(model::BragiError::from)?;
    if results.is_empty() {
        query(
            &q,
            &pt_datasets,
            &poi_datasets,
            all_data,
            &mut rubber,
            MatchType::Fuzzy,
            offset,
            limit,
            coord,
            shape,
            &types,
            &zone_types,
            &poi_types,
            &langs,
            debug,
        )
        .map_err(model::BragiError::from)
    } else {
        Ok(results)
    }
}
