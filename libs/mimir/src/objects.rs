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
use serde;
use std::rc::Rc;
use std::cell::Cell;
use serde::ser::{Serializer, SerializeStruct};
use std::cmp::Ordering;
use std::fmt;

pub trait Incr: Clone {
    fn id(&self) -> &str;
    fn incr(&mut self);
}

/// Object stored in elastic search
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Place {
    Admin(Admin),
    Street(Street),
    Addr(Addr),
    Poi(Poi),
    Stop(Stop),
}

impl Place {
    pub fn is_admin(&self) -> bool {
        match *self {
            Place::Admin(_) => true,
            _ => false,
        }
    }
    pub fn is_street(&self) -> bool {
        match *self {
            Place::Street(_) => true,
            _ => false,
        }
    }
    pub fn is_addr(&self) -> bool {
        match *self {
            Place::Addr(_) => true,
            _ => false,
        }
    }
    pub fn is_poi(&self) -> bool {
        match *self {
            Place::Poi(_) => true,
            _ => false,
        }
    }
    pub fn is_stop(&self) -> bool {
        match *self {
            Place::Stop(_) => true,
            _ => false,
        }
    }
    pub fn poi(&self) -> Option<&Poi> {
        match *self {
            Place::Poi(ref poi) => Some(poi),
            _ => None,
        }
    }

    pub fn label(&self) -> &str {
        match *self {
            Place::Admin(ref o) => o.label(),
            Place::Street(ref o) => o.label(),
            Place::Addr(ref o) => o.label(),
            Place::Poi(ref o) => o.label(),
            Place::Stop(ref o) => o.label(),
        }
    }

    pub fn admins(&self) -> Vec<Rc<Admin>> {
        match *self {
            Place::Admin(ref o) => o.admins(),
            Place::Street(ref o) => o.admins(),
            Place::Addr(ref o) => o.admins(),
            Place::Poi(ref o) => o.admins(),
            Place::Stop(ref o) => o.admins(),
        }
    }
}

pub trait MimirObject: serde::Serialize {
    fn is_geo_data() -> bool;
    fn doc_type() -> &'static str; // provides the elasticsearch type name
    fn es_id(&self) -> Option<String>; // provides the elasticsearch id
}

pub trait Members {
    fn label(&self) -> &str;
    fn admins(&self) -> Vec<Rc<Admin>>;
}

impl<'a, T: MimirObject> MimirObject for &'a T {
    fn is_geo_data() -> bool {
        T::is_geo_data()
    }
    fn doc_type() -> &'static str {
        T::doc_type()
    }
    fn es_id(&self) -> Option<String> {
        T::es_id(self)
    }
}

impl<T: MimirObject> MimirObject for Rc<T> {
    fn is_geo_data() -> bool {
        T::is_geo_data()
    }
    fn doc_type() -> &'static str {
        T::doc_type()
    }
    fn es_id(&self) -> Option<String> {
        T::es_id(self)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Poi {
    pub id: String,
    pub label: String,
    pub name: String,
    pub coord: Coord,
    pub administrative_regions: Vec<Rc<Admin>>,
    pub weight: f64,
    pub zip_codes: Vec<String>,
    pub poi_type: PoiType,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PoiType {
    pub id: String,
    pub name: String,
}

impl MimirObject for Poi {
    fn is_geo_data() -> bool {
        true
    }
    fn doc_type() -> &'static str {
        "poi"
    }
    fn es_id(&self) -> Option<String> {
        Some(self.id.clone())
    }
}

impl Members for Poi {
    fn label(&self) -> &str {
        &self.label
    }
    fn admins(&self) -> Vec<Rc<Admin>> {
        self.administrative_regions.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Stop {
    pub id: String,
    pub label: String,
    pub name: String,
    pub coord: Coord,
    pub administrative_regions: Vec<Rc<Admin>>,
    pub weight: f64,
    pub zip_codes: Vec<String>,
}

impl MimirObject for Stop {
    fn is_geo_data() -> bool {
        false
    }
    fn doc_type() -> &'static str {
        "stop"
    }
    fn es_id(&self) -> Option<String> {
        Some(self.id.clone())
    }
}
impl Members for Stop {
    fn label(&self) -> &str {
        &self.label
    }
    fn admins(&self) -> Vec<Rc<Admin>> {
        self.administrative_regions.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Admin {
    pub id: String,
    pub insee: String,
    pub level: u32,
    pub label: String,
    pub name: String,
    pub zip_codes: Vec<String>,
    pub weight: Cell<f64>,
    pub coord: Coord,
    #[serde(serialize_with="custom_multi_polygon_serialize",
            deserialize_with="custom_multi_polygon_deserialize",
            skip_serializing_if="Option::is_none",
            default)]
    pub boundary: Option<geo::MultiPolygon<f64>>,
}

fn custom_multi_polygon_serialize<S>(multi_polygon_option: &Option<geo::MultiPolygon<f64>>,
                                     serializer: S)
                                     -> Result<S::Ok, S::Error>
    where S: Serializer
{
    use geojson::{Value, Geometry, GeoJson};
    use serde::Serialize;

    match *multi_polygon_option {
        Some(ref multi_polygon) => {
            GeoJson::Geometry(Geometry::new(Value::from(multi_polygon))).serialize(serializer)
        }
        None => serializer.serialize_none(),
    }
}

fn custom_multi_polygon_deserialize<D>(d: D) -> Result<Option<geo::MultiPolygon<f64>>, D::Error>
    where D: serde::de::Deserializer
{
    use geojson;
    use serde::Deserialize;
    use geojson::conversion::TryInto;

    Option::<geojson::GeoJson>::deserialize(d).map(|option| {
        option.and_then(|geojson| match geojson {
            geojson::GeoJson::Geometry(geojson_geom) => {
                let geo_geom: Result<geo::Geometry<f64>, _> = geojson_geom.value.try_into();
                match geo_geom {
                    Ok(geo::Geometry::MultiPolygon(geo_multi_polygon)) => Some(geo_multi_polygon),
                    Ok(_) => None,
                    Err(e) => {
                        warn!("Error deserializing geometry: {}", e);
                        None
                    }
                }
            }
            _ => None,
        })
    })
}

impl Ord for Admin {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl PartialOrd for Admin {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Admin {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Members for Admin {
    fn label(&self) -> &str {
        &self.label
    }
    fn admins(&self) -> Vec<Rc<Admin>> {
        vec![Rc::new(self.clone())]
    }
}

impl Eq for Admin {}

impl MimirObject for Admin {
    fn is_geo_data() -> bool {
        true
    }
    fn doc_type() -> &'static str {
        "admin"
    }
    fn es_id(&self) -> Option<String> {
        Some(self.id.clone())
    }
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Street {
    pub id: String,
    pub street_name: String,
    pub administrative_regions: Vec<Rc<Admin>>,
    pub label: String,
    pub weight: f64,
    pub coord: Coord,
    pub zip_codes: Vec<String>,
}
impl Incr for Street {
    fn id(&self) -> &str {
        &self.id
    }
    fn incr(&mut self) {
        self.weight += 1.;
    }
}

impl MimirObject for Street {
    fn is_geo_data() -> bool {
        true
    }
    fn doc_type() -> &'static str {
        "street"
    }
    fn es_id(&self) -> Option<String> {
        Some(self.id.clone())
    }
}

impl Members for Street {
    fn label(&self) -> &str {
        &self.label
    }
    fn admins(&self) -> Vec<Rc<Admin>> {
        self.administrative_regions.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Addr {
    pub id: String,
    pub house_number: String,
    pub street: Street,
    pub label: String,
    pub coord: Coord,
    pub weight: f64,
    pub zip_codes: Vec<String>,
}

impl MimirObject for Addr {
    fn is_geo_data() -> bool {
        true
    }
    fn doc_type() -> &'static str {
        "addr"
    }
    fn es_id(&self) -> Option<String> {
        Some(self.id.clone())
    }
}

impl Members for Addr {
    fn label(&self) -> &str {
        &self.label
    }
    fn admins(&self) -> Vec<Rc<Admin>> {
        self.street.admins()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AliasOperations {
    pub actions: Vec<AliasOperation>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AliasOperation {
    #[serde(skip_serializing_if="Option::is_none")]
    pub add: Option<AliasParameter>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub remove: Option<AliasParameter>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AliasParameter {
    pub index: String,
    pub alias: String,
}

// we want a custom serialization for coords, and so far the cleanest way
// to do this that has been found is to wrap the coord in another struct
#[derive(Debug, Clone)]
pub struct Coord(pub geo::Coordinate<f64>);
impl Coord {
    pub fn new(lat: f64, lon: f64) -> Coord {
        Coord(geo::Coordinate { x: lat, y: lon })
    }
    pub fn lat(&self) -> f64 {
        self.x
    }
    pub fn lon(&self) -> f64 {
        self.y
    }
    pub fn is_default(&self) -> bool {
        self.lat() == 0. && self.lon() == 0.
    }
    pub fn is_valid(&self) -> bool {
        !self.is_default() && -90. <= self.lat() && self.lat() <= 90. && -180. <= self.lon() &&
        self.lon() <= 180.
    }
}
impl Default for Coord {
    fn default() -> Coord {
        Coord(geo::Coordinate { x: 0., y: 0. })
    }
}

impl ::std::ops::Deref for Coord {
    type Target = geo::Coordinate<f64>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl serde::Serialize for Coord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: serde::Serializer
    {
        let mut ser = serializer.serialize_struct("Coord", 2)?;
        ser.serialize_field("lat", &self.0.x)?;
        ser.serialize_field("lon", &self.0.y)?;
        ser.end()
    }
}

enum GeoCoordField {
    X,
    Y,
}

impl serde::Deserialize for GeoCoordField {
    fn deserialize<D>(deserializer: D) -> Result<GeoCoordField, D::Error>
        where D: serde::de::Deserializer
    {
        struct GeoCoordFieldVisitor;

        impl serde::de::Visitor for GeoCoordFieldVisitor {
            type Value = GeoCoordField;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("`lat` or `lon`")
            }

            fn visit_str<E>(self, value: &str) -> Result<GeoCoordField, E>
                where E: serde::de::Error
            {
                match value {
                    "lat" => Ok(GeoCoordField::X),
                    "lon" => Ok(GeoCoordField::Y),
                    _ => Err(serde::de::Error::unknown_field(value, GEOCOORDFIELDS)),
                }
            }
        }

        deserializer.deserialize(GeoCoordFieldVisitor)
    }
}
const GEOCOORDFIELDS: &'static [&'static str] = &["lat", "lon"];

impl serde::Deserialize for Coord {
    fn deserialize<D>(deserializer: D) -> Result<Coord, D::Error>
        where D: serde::de::Deserializer
    {
        deserializer.deserialize_struct("Coord", GEOCOORDFIELDS, GeoCoordDeserializerVisitor)
    }
}
struct GeoCoordDeserializerVisitor;

impl serde::de::Visitor for GeoCoordDeserializerVisitor {
    type Value = Coord;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("struct Coord")
    }

    fn visit_map<V>(self, mut visitor: V) -> Result<Coord, V::Error>
        where V: serde::de::MapVisitor
    {
        use serde::de::Error;
        let mut x = None;
        let mut y = None;

        while let Some(key) = visitor.visit_key()? {
            match key {
                GeoCoordField::X => {
                    if x.is_some() {
                        return Err(Error::duplicate_field("x"));
                    }
                    x = Some(visitor.visit_value()?);
                }
                GeoCoordField::Y => {
                    if y.is_some() {
                        return Err(Error::duplicate_field("y"));
                    }
                    y = Some(visitor.visit_value()?);
                }
            }
        }

        let x = match x {
            Some(x) => x,
            None => return Err(Error::missing_field("x")),
        };

        let y = match y {
            Some(y) => y,
            None => return Err(Error::missing_field("y")),
        };

        Ok(Coord(geo::Coordinate { x: x, y: y }))
    }
}
