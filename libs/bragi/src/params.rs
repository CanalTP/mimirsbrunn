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

use rustless::json::JsonValue;
use std::str::FromStr;
use valico::common::error as valico_error;
use valico::json_dsl::{self, Builder, Param};

const MAX_LAT: f64 = 180f64;
const MIN_LAT: f64 = -180f64;

const MAX_LON: f64 = 90f64;
const MIN_LON: f64 = -90f64;

static ZONE_TYPES: &'static [&str] = &[
    "suburb",
    "city",
    "state_district",
    "state",
    "country_region",
    "country",
    "non_admin",
];

pub fn dataset_param(params: &mut Builder) {
    params.opt("pt_dataset", |t| {
        t.coerce(json_dsl::encoded_array(","));
    });
    params.opt_typed("_all_data", json_dsl::boolean());
}

pub fn coord_param(params: &mut Builder, is_opt: bool) {
    fn checker<F: FnOnce(&mut Param)>(
        builder: &mut Builder,
        is_opt: bool,
        name: &str,
        param_builder: F,
    ) {
        if is_opt {
            builder.opt(name, param_builder)
        } else {
            builder.req(name, param_builder)
        }
    }
    checker(params, is_opt, "lon", |lon| {
        lon.coerce(json_dsl::f64());
        lon.validate_with(|val, path| {
            check_bound(val, path, MIN_LON, MAX_LON, "lon is not a valid longitude")
        });
    });

    checker(params, is_opt, "lat", |lat| {
        lat.coerce(json_dsl::f64());
        lat.validate_with(|val, path| {
            check_bound(val, path, MIN_LAT, MAX_LAT, "lat is not a valid latitude")
        });
    });
    params.validate_with(|val, path| {
        // if we have a lat we should have a lon (and the opposite)
        if let Some(obj) = val.as_object() {
            let has_lon = obj.get("lon").is_some();
            let has_lat = obj.get("lat").is_some();
            if has_lon ^ has_lat {
                Err(vec![Box::new(json_dsl::errors::WrongValue {
                    path: path.to_string(),
                    detail: Some(
                        "you need to provide a lon AND a lat \
                         if you provide one of them"
                            .to_string(),
                    ),
                })])
            } else {
                Ok(())
            }
        } else {
            unreachable!("should never happen, already checked");
        }
    });
}

pub fn paginate_param(params: &mut Builder) {
    params.opt_typed("limit", json_dsl::u64());
    params.opt_typed("offset", json_dsl::u64());
}

pub fn shape_param(params: &mut Builder) {
    params.req("shape", |shape| {
        shape.coerce(json_dsl::object());
        shape.nest(|params| {
            params.req("type", |geojson_type| {
                geojson_type.coerce(json_dsl::string());
                geojson_type.allow_values(&["Feature".to_string()]);
            });
            params.req("geometry", |geometry| {
                geometry.coerce(json_dsl::object());
                geometry.nest(|params| {
                    params.req("type", |geojson_type| {
                        geojson_type.coerce(json_dsl::string());
                        geojson_type.allow_values(&["Polygon".to_string()]);
                    });
                });
                geometry.nest(|params| {
                    params.req("coordinates", |shape| {
                        shape.coerce(json_dsl::array());
                        shape.validate_with(|val, path| {
                            check_coordinates(val, path, "Coordinates is invalid")
                        });
                    });
                });
            });
        });
    });
}

pub fn types_param(params: &mut Builder) {
    params.opt("type", |t| {
        t.coerce(json_dsl::encoded_array(","));
        t.validate_with(|val, path| check_type(val.as_array().unwrap(), path));
    });
}

pub fn zone_type_param(params: &mut Builder) {
    params.opt("zone_type", |t| {
        t.coerce(json_dsl::string());
        t.validate_with(|val, path| check_zone_type(val.as_str().unwrap(), path));
    });
}

pub fn get_param_array<'a>(params: &'a JsonValue, param_name: &str) -> Vec<&'a str> {
    params
        .find(param_name)
        .and_then(|val| val.as_array())
        .map(|val| val.iter().map(|val| val.as_str().unwrap()).collect())
        .unwrap_or(vec![])
}

pub fn get_zone_type<'a>(params: &'a JsonValue) -> Option<&'a str> {
    let zt = params
        .find("zone_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if zt.is_empty() {
        None
    } else {
        Some(zt)
    }
}

fn check_bound(
    val: &JsonValue,
    path: &str,
    min: f64,
    max: f64,
    error_msg: &str,
) -> Result<(), valico_error::ValicoErrors> {
    if let Some(lon) = val.as_f64() {
        if min <= lon && lon <= max {
            Ok(())
        } else {
            Err(vec![Box::new(json_dsl::errors::WrongValue {
                path: path.to_string(),
                detail: Some(error_msg.to_string()),
            })])
        }
    } else {
        unreachable!("should never happen, already checked");
    }
}

fn check_type(types: &[JsonValue], path: &str) -> Result<(), valico_error::ValicoErrors> {
    for type_ in types {
        if let Err(e) = Type::from_str(type_.as_str().unwrap()) {
            return Err(vec![Box::new(json_dsl::errors::WrongValue {
                path: path.to_string(),
                detail: Some(e),
            })]);
        }
    }

    Ok(())
}

fn check_zone_type(zone_type: &str, path: &str) -> Result<(), valico_error::ValicoErrors> {
    type Err = String;
    fn is_valid(s: &str) -> Result<(), Err> {
        if ZONE_TYPES.contains(&s) {
            Ok(())
        } else {
            Err(format!(
                "{} does not belong to the valid zone types list: {}",
                s,
                ZONE_TYPES.join(", ")
            ))
        }
    }
    match is_valid(zone_type) {
        Ok(_) => Ok(()),
        Err(e) => Err(vec![Box::new(json_dsl::errors::WrongValue {
            path: path.to_string(),
            detail: Some(e),
        })]),
    }
}

fn check_coordinates(
    val: &JsonValue,
    path: &str,
    error_msg: &str,
) -> Result<(), valico_error::ValicoErrors> {
    if !val.is_array() {
        return Err(vec![Box::new(json_dsl::errors::WrongType {
            path: path.to_string(),
            detail: error_msg.to_string(),
        })]);
    }
    let array = val.as_array().unwrap();
    if array.is_empty() {
        return Err(vec![Box::new(json_dsl::errors::WrongValue {
            path: path.to_string(),
            detail: Some(error_msg.to_string()),
        })]);
    }

    for arr0 in array {
        if !arr0.is_array() {
            return Err(vec![Box::new(json_dsl::errors::WrongType {
                path: path.to_string(),
                detail: error_msg.to_string(),
            })]);
        }
        let arr1 = arr0.as_array().unwrap();
        if arr1.is_empty() {
            return Err(vec![Box::new(json_dsl::errors::WrongValue {
                path: path.to_string(),
                detail: Some(error_msg.to_string()),
            })]);
        }
        for arr2 in arr1 {
            if !arr2.is_array() {
                return Err(vec![Box::new(json_dsl::errors::WrongType {
                    path: path.to_string(),
                    detail: error_msg.to_string(),
                })]);
            }
            let lonlat = arr2.as_array().unwrap();
            if lonlat.len() != 2 {
                return Err(vec![Box::new(json_dsl::errors::WrongValue {
                    path: path.to_string(),
                    detail: Some(error_msg.to_string()),
                })]);
            }

            if !(lonlat[0].is_f64() && lonlat[1].is_f64()) {
                return Err(vec![Box::new(json_dsl::errors::WrongType {
                    path: path.to_string(),
                    detail: error_msg.to_string(),
                })]);
            }
            let lon = lonlat[0].as_f64().unwrap();
            let lat = lonlat[1].as_f64().unwrap();
            if !(MIN_LON <= lon && lon <= MAX_LON) {
                return Err(vec![Box::new(json_dsl::errors::WrongValue {
                    path: path.to_string(),
                    detail: Some(error_msg.to_string()),
                })]);
            }
            if !(MIN_LAT <= lat && lat <= MAX_LAT) {
                return Err(vec![Box::new(json_dsl::errors::WrongValue {
                    path: path.to_string(),
                    detail: Some(error_msg.to_string()),
                })]);
            }
        }
    }
    Ok(())
}

#[derive(Copy, Clone, Debug)]
enum Type {
    Zone,
    House,
    Poi,
    StopArea,
    Street,
}

impl FromStr for Type {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "city" => Ok(Type::Zone),
            "zone" => Ok(Type::Zone),
            "house" => Ok(Type::House),
            "poi" => Ok(Type::Poi),
            "public_transport:stop_area" => Ok(Type::StopArea),
            "street" => Ok(Type::Street),
            _ => Err(format!("{} is not a valid type", s)),
        }
    }
}
