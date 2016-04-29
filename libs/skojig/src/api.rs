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
use rustless;
use serde;
use serde_json;
use rustc_serialize::json;
use rustless::server::status;
use rustless::{Api, Nesting, Versioning};
use valico::json_dsl;
use valico::common::error as valico_error;
use super::query;
use model::v1::*;
use model;

fn render<T>(mut client: rustless::Client,
             obj: T)
             -> Result<rustless::Client, rustless::ErrorResponse>
    where T: serde::Serialize
{
    client.set_json_content_type();
    client.text(serde_json::to_string(&obj).unwrap())
}

pub fn root() -> rustless::Api {
    Api::build(|api| {
        api.get("", |endpoint| {
            endpoint.handle(|client, _params| {
                let desc = EndPoint { description: "autocomplete service".to_string() };
                render(client, desc)
            })
        });

        api.error_formatter(|error, _media| {
            let err = if error.is::<rustless::errors::Validation>() {
                let val_err = error.downcast::<rustless::errors::Validation>().unwrap();
                //TODO better message, we shouldn't use {:?} but access the `path` and `detail` of all errrors in val_err.reason
                CustomError {short: "validation error".to_string(), long: format!("invalid arguments {:?}", val_err.reason)}
            } else {
                CustomError {short: "bad_request".to_string(), long: format!("bad request, error: {}", error)}
            };
            let mut resp = rustless::Response::from(status::StatusCode::BadRequest, Box::new(serde_json::to_string(&err).unwrap()));
            resp.set_json_content_type();
            Some(resp)
        });
        api.mount(v1());
    })
}

pub fn v1() -> rustless::Api {
    Api::build(|api| {
        api.version("v1", Versioning::Path);

        api.get("", |endpoint| {
            endpoint.desc("api interface version v1");
            endpoint.handle(|client, _params| {
                render(client,
                       V1Reponse::Response { description: "api version 1".to_string() })
            })
        });
        api.mount(status());
        api.mount(autocomplete());
    })
}

pub fn status() -> rustless::Api {
    Api::build(|api| {
        api.get("status", |endpoint| {
            endpoint.handle(|client, _params| {
                let status = Status {
                    version: "14".to_string(),
                    status: "good".to_string(),
                };
                render(client, status)
            })
        });
    })
}

fn check_bound(val: &json::Json,
               path: &str,
               min: f64,
               max: f64,
               error_msg: &str)
               -> Result<(), valico_error::ValicoErrors> {
    if let json::Json::F64(lon) = *val {
        if min <= lon && lon <= max {
            Ok(())
        } else {
            Err(vec![Box::new(json_dsl::errors::WrongValue {
                         path: path.to_string(),
                         detail: Some(error_msg.to_string()),
                     })])
        }
    } else {
        panic!("should never happen, already checked");
    }
}

pub fn autocomplete() -> rustless::Namespace {
    rustless::Namespace::build("autocomplete", |ns| {
        ns.get("", |endpoint| {
            endpoint.params(|params| {
                params.req_typed("q", json_dsl::string());

                params.opt("lon", |lon| {
                    lon.coerce(json_dsl::f64());
                    lon.validate_with(|val, path| {
                        check_bound(val, path, -180f64, 180f64, "lon is not a valid longitude")
                    });
                });

                params.opt("lat", |lat| {
                    lat.coerce(json_dsl::f64());
                    lat.validate_with(|val, path| {
                        check_bound(val, path, -90f64, 900f64, "lat is not a valid latitude")
                    });
                });
                params.validate_with(|val, path| {
                    // if we have a lat we should have a lon (and the opposite)
                    if let json::Json::Object(ref obj) = *val {
                        let has_lon = obj.get("lon").is_some();
                        let has_lat = obj.get("lat").is_some();
                        if has_lon ^ has_lat {
                            Err(vec![Box::new(json_dsl::errors::WrongValue {
                                         path: path.to_string(),
                                         detail: Some("you need to provide a lon AND a lat if \
                                                       you provide one of them"
                                                          .to_string()),
                                     })])
                        } else {
                            Ok(())
                        }
                    } else {
                        panic!("should never happen, already checked");
                    }
                });
            });
            endpoint.handle(|client, params| {
                let q = params.find("q").unwrap().as_string().unwrap().to_string();
                let lon = params.find("lon").and_then(|p| p.as_f64());
                let lat = params.find("lat").and_then(|p| p.as_f64());
                // we have already checked that if there is a lon, lat is not None, so we can unwrap
                let coord = lon.and_then(|lon| Some(model::Coord{lon: lon, lat: lat.unwrap()}));
                let autocomplete = query::autocomplete(q, coord);
                render(client, autocomplete)
            })
        });
    })
}
