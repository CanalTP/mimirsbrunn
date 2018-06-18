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

use geo;
use geojson;
use heck::SnakeCase;
use mimir;
use rs_es::error::EsError;
use std::rc::Rc;

#[derive(Fail, Debug)]
pub enum BragiError {
    #[fail(display = "Unable to find object")]
    ObjectNotFound,
    #[fail(display = "Impossible to find object")]
    IndexNotFound,
    #[fail(display = "invalid query {}", _0)]
    Es(EsError),
}

impl From<EsError> for BragiError {
    fn from(e: EsError) -> Self {
        BragiError::Es(e)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Geocoding {
    version: String,
    query: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Feature {
    #[serde(rename = "type")]
    pub feature_type: String,
    pub geometry: geojson::Geometry,
    pub properties: Properties,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Properties {
    pub geocoding: GeocodingResponse,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct GeocodingResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub place_type: String, // FIXME: use an enum?
    pub label: Option<String>,
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub housenumber: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street: Option<String>,
    pub postcode: Option<String>,
    pub city: Option<String>,
    pub citycode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    // pub accuracy: Option<i32>,
    // pub district: Option<String>,
    // pub county: Option<String>,
    // pub state: Option<String>,
    // pub country: Option<String>,
    // pub geohash: Option<String>,
    pub administrative_regions: Vec<Rc<mimir::Admin>>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub poi_types: Vec<mimir::PoiType>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub properties: Vec<mimir::Property>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Box<GeocodingResponse>>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub commercial_modes: Vec<mimir::CommercialMode>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub comments: Vec<mimir::Comment>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub physical_modes: Vec<mimir::PhysicalMode>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub timezone: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub codes: Vec<mimir::Code>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub feed_publishers: Vec<mimir::FeedPublisher>,
}

trait ToGeom {
    fn to_geom(&self) -> geojson::Geometry;
}

impl ToGeom for mimir::Place {
    fn to_geom(&self) -> geojson::Geometry {
        match self {
            &mimir::Place::Admin(ref admin) => admin.coord.to_geom(),
            &mimir::Place::Street(ref street) => street.coord.to_geom(),
            &mimir::Place::Addr(ref addr) => addr.coord.to_geom(),
            &mimir::Place::Poi(ref poi) => poi.coord.to_geom(),
            &mimir::Place::Stop(ref stop) => stop.coord.to_geom(),
        }
    }
}

impl ToGeom for geo::Coordinate<f64> {
    fn to_geom(&self) -> geojson::Geometry {
        geojson::Geometry::new(geojson::Value::Point(vec![self.x, self.y]))
    }
}

impl From<mimir::Place> for Feature {
    fn from(other: mimir::Place) -> Feature {
        let geom = other.to_geom();
        let geocoding = match other {
            mimir::Place::Admin(admin) => GeocodingResponse::from(admin),
            mimir::Place::Street(street) => GeocodingResponse::from(street),
            mimir::Place::Addr(addr) => GeocodingResponse::from(addr),
            mimir::Place::Poi(poi) => GeocodingResponse::from(poi),
            mimir::Place::Stop(poi) => GeocodingResponse::from(poi),
        };
        Feature {
            feature_type: "Feature".to_string(),
            geometry: geom,
            properties: Properties {
                geocoding: geocoding,
            },
        }
    }
}

impl From<mimir::Admin> for GeocodingResponse {
    fn from(other: mimir::Admin) -> GeocodingResponse {
        let type_ = get_admin_type(&other);
        let name = Some(other.name);
        let insee = Some(other.insee);
        let level = Some(other.level); //might be used for type_ and become useless
        let postcode = if other.zip_codes.is_empty() {
            None
        } else {
            Some(other.zip_codes.join(";"))
        };
        let label = Some(other.label);
        GeocodingResponse {
            id: other.id,
            citycode: insee,
            level: level,
            place_type: type_,
            name: name,
            postcode: postcode,
            label: label,
            ..Default::default()
        }
    }
}

fn get_admin_type(adm: &mimir::Admin) -> String {
    match adm.zone_type {
        Some(t) => format!("{:?}", t).to_snake_case(),
        // TODO below return administrative_region when admin_type is removed
        None => adm.admin_type.to_string(),
    }
}

fn get_city_name(admins: &Vec<Rc<mimir::Admin>>) -> Option<String> {
    admins
        .iter()
        .find(|a| a.is_city())
        .map(|admin| admin.name.clone())
}

fn get_citycode(admins: &Vec<Rc<mimir::Admin>>) -> Option<String> {
    admins
        .iter()
        .find(|a| a.is_city())
        .map(|admin| admin.insee.clone())
}

impl From<mimir::Street> for GeocodingResponse {
    fn from(other: mimir::Street) -> GeocodingResponse {
        let type_ = "street".to_string();
        let name = Some(other.name);
        let label = Some(other.label);
        let admins = other.administrative_regions;
        let city = get_city_name(&admins);
        let postcode = if other.zip_codes.is_empty() {
            None
        } else {
            Some(other.zip_codes.join(";"))
        };
        let citycode = get_citycode(&admins);

        GeocodingResponse {
            id: other.id,
            citycode: citycode,
            place_type: type_,
            name: name.clone(),
            postcode: postcode,
            label: label,
            street: name,
            city: city,
            administrative_regions: admins,
            ..Default::default()
        }
    }
}

impl From<mimir::Addr> for GeocodingResponse {
    fn from(other: mimir::Addr) -> GeocodingResponse {
        let type_ = "house".to_string();
        let label = Some(other.label);
        let housenumber = Some(other.house_number.to_string());
        let street_name = Some(other.street.name.to_string());
        let name = Some(format!("{} {}", other.house_number, other.street.name));
        let admins = other.street.administrative_regions;
        let city = get_city_name(&admins);
        let postcode = if other.zip_codes.is_empty() {
            None
        } else {
            Some(other.zip_codes.join(";"))
        };
        let citycode = get_citycode(&admins);

        GeocodingResponse {
            id: other.id,
            citycode: citycode,
            place_type: type_,
            name: name,
            postcode: postcode,
            label: label,
            housenumber: housenumber,
            street: street_name,
            city: city,
            administrative_regions: admins,
            ..Default::default()
        }
    }
}

impl From<mimir::Poi> for GeocodingResponse {
    fn from(other: mimir::Poi) -> GeocodingResponse {
        let type_ = "poi".to_string();
        let label = Some(other.label);
        let name = Some(other.name);
        let admins = other.administrative_regions;
        let city = get_city_name(&admins);
        let postcode = if other.zip_codes.is_empty() {
            None
        } else {
            Some(other.zip_codes.join(";"))
        };
        let citycode = get_citycode(&admins);

        GeocodingResponse {
            id: other.id,
            citycode: citycode,
            place_type: type_,
            name: name,
            postcode: postcode,
            label: label,
            city: city,
            administrative_regions: admins,
            poi_types: vec![other.poi_type],
            properties: other.properties,
            address: match other.address {
                Some(mimir::Address::Addr(addr)) => Some(Box::new(GeocodingResponse::from(addr))),
                Some(mimir::Address::Street(street)) => {
                    Some(Box::new(GeocodingResponse::from(street)))
                }
                _ => None,
            },
            ..Default::default()
        }
    }
}

impl From<mimir::Stop> for GeocodingResponse {
    fn from(other: mimir::Stop) -> GeocodingResponse {
        let type_ = "public_transport:stop_area".to_string();
        let label = Some(other.label);
        let name = Some(other.name);
        let admins = other.administrative_regions;
        let city = get_city_name(&admins);
        let postcode = if other.zip_codes.is_empty() {
            None
        } else {
            Some(other.zip_codes.join(";"))
        };
        let citycode = get_citycode(&admins);

        GeocodingResponse {
            id: other.id,
            citycode: citycode,
            place_type: type_,
            name: name,
            postcode: postcode,
            label: label,
            city: city,
            administrative_regions: admins,
            commercial_modes: other.commercial_modes,
            physical_modes: other.physical_modes,
            comments: other.comments,
            timezone: Some(other.timezone),
            codes: other.codes,
            properties: other.properties,
            feed_publishers: other.feed_publishers,
            ..Default::default()
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Autocomplete {
    #[serde(rename = "type")]
    format_type: String,
    geocoding: Geocoding,
    features: Vec<Feature>,
}

impl Autocomplete {
    pub fn new(q: String, features: Vec<Feature>) -> Autocomplete {
        // TODO couldn't we mode this function ? in Autocomplete ?
        Autocomplete {
            format_type: "FeatureCollection".to_string(),
            geocoding: Geocoding {
                version: "0.1.0".to_string(),
                query: Some(q),
            },
            features: features,
        }
    }
}

impl From<Vec<mimir::Place>> for Autocomplete {
    fn from(places: Vec<mimir::Place>) -> Autocomplete {
        Autocomplete::new(
            "".to_string(),
            places.into_iter().map(|p| Feature::from(p)).collect(),
        )
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Coord {
    pub lat: f64,
    pub lon: f64,
}

pub mod v1 {
    use super::BragiError;
    use iron;
    use mimir;
    use rs_es;

    pub trait HasStatus {
        fn status(&self) -> iron::status::Status;
    }

    // Note: I think this should be in api.rs but with the serde stuff it's easier for all
    // serde struct to be in the same file

    #[derive(Serialize, Deserialize, Debug)]
    pub struct EndPoint {
        pub description: String,
    }

    impl HasStatus for EndPoint {
        fn status(&self) -> iron::status::Status {
            default_status()
        }
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct CustomError {
        pub short: String,
        pub long: String,
        #[serde(skip, default = "default_status")]
        pub status: iron::status::Status,
    }

    fn default_status() -> iron::status::Status {
        iron::status::Status::Ok
    }

    #[derive(Debug)]
    pub enum AutocompleteResponse {
        Error(CustomError),
        Autocomplete(super::Autocomplete),
    }

    impl HasStatus for AutocompleteResponse {
        fn status(&self) -> iron::status::Status {
            match self {
                AutocompleteResponse::Error(e) => e.status,
                AutocompleteResponse::Autocomplete(_) => iron::status::Status::Ok,
            }
        }
    }

    use serde;
    impl serde::Serialize for AutocompleteResponse {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            match self {
                &AutocompleteResponse::Autocomplete(ref a) => serializer.serialize_some(a),
                &AutocompleteResponse::Error(ref e) => serializer.serialize_some(e),
            }
        }
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Status {
        pub version: String,
        pub es: String,
        pub status: String,
    }

    impl HasStatus for Status {
        fn status(&self) -> iron::status::Status {
            iron::status::Status::Ok
        }
    }

    impl From<Result<Vec<mimir::Place>, BragiError>> for AutocompleteResponse {
        fn from(r: Result<Vec<mimir::Place>, BragiError>) -> AutocompleteResponse {
            match r {
                Ok(places) => AutocompleteResponse::Autocomplete(super::Autocomplete::from(places)),
                Err(e) => AutocompleteResponse::Error(CustomError {
                    short: "query error".to_string(),
                    long: format!("{}", e),
                    status: match e {
                        BragiError::ObjectNotFound => iron::status::Status::NotFound,
                        BragiError::IndexNotFound => iron::status::Status::NotFound,
                        BragiError::Es(es_error) => match es_error {
                            rs_es::error::EsError::HttpError(_) => {
                                iron::status::Status::ServiceUnavailable
                            }
                            _ => iron::status::Status::InternalServerError,
                        },
                    },
                }),
            }
        }
    }
}
