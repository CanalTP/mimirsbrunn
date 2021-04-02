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
use cosmogony::ZoneType;
use geo_types::{Coordinate, MultiPolygon, Rect};
use geojson::Geometry;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};
use slog_scope::warn;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::iter::FromIterator;
use std::rc::Rc;
use std::sync::Arc;
use transit_model::objects::Rgb;
use typed_index_collection::Idx;

pub trait Incr: Clone {
    fn id(&self) -> &str;
    fn incr(&mut self);
}

/// Object stored in elastic search
#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Place {
    Admin(Admin),
    Street(Street),
    Addr(Addr),
    Poi(Poi),
    Stop(Stop),
}

/// Object stored in elastic search
#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Address {
    Street(Street),
    Addr(Addr),
}

impl Place {
    pub fn is_admin(&self) -> bool {
        matches!(self, Place::Admin(_))
    }

    pub fn is_street(&self) -> bool {
        matches!(self, Place::Street(_))
    }

    pub fn is_addr(&self) -> bool {
        matches!(self, Place::Addr(_))
    }

    pub fn is_poi(&self) -> bool {
        matches!(self, Place::Poi(_))
    }

    pub fn is_stop(&self) -> bool {
        matches!(self, Place::Stop(_))
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

    pub fn admins(&self) -> Vec<Arc<Admin>> {
        match *self {
            Place::Admin(ref o) => o.admins(),
            Place::Street(ref o) => o.admins(),
            Place::Addr(ref o) => o.admins(),
            Place::Poi(ref o) => o.admins(),
            Place::Stop(ref o) => o.admins(),
        }
    }

    pub fn address(&self) -> Option<Address> {
        match *self {
            Place::Admin(_) => None,
            Place::Street(ref o) => Some(Address::Street(o.clone())),
            Place::Addr(ref o) => Some(Address::Addr(o.clone())),
            Place::Poi(_) => None,
            Place::Stop(_) => None,
        }
    }

    pub fn distance(&self) -> Option<u32> {
        match *self {
            Place::Admin(ref o) => o.distance,
            Place::Street(ref o) => o.distance,
            Place::Addr(ref o) => o.distance,
            Place::Poi(ref o) => o.distance,
            Place::Stop(ref o) => o.distance,
        }
    }

    pub fn set_distance(&mut self, d: u32) {
        match self {
            Place::Admin(ref mut o) => o.distance = Some(d),
            Place::Street(ref mut o) => o.distance = Some(d),
            Place::Addr(ref mut o) => o.distance = Some(d),
            Place::Poi(ref mut o) => o.distance = Some(d),
            Place::Stop(ref mut o) => o.distance = Some(d),
        }
    }

    pub fn coord(&self) -> &Coord {
        match self {
            Place::Admin(ref o) => &o.coord,
            Place::Street(ref o) => &o.coord,
            Place::Addr(ref o) => &o.coord,
            Place::Poi(ref o) => &o.coord,
            Place::Stop(ref o) => &o.coord,
        }
    }

    pub fn set_context(&mut self, context: Context) {
        match self {
            Place::Admin(ref mut o) => o.context = Some(context),
            Place::Street(ref mut o) => o.context = Some(context),
            Place::Addr(ref mut o) => o.context = Some(context),
            Place::Poi(ref mut o) => o.context = Some(context),
            Place::Stop(ref mut o) => o.context = Some(context),
        }
    }

    /* We can afford to clone the context because we're in debug mode
     * and performance are less critical */
    pub fn context(&self) -> Option<Context> {
        match self {
            Place::Admin(ref o) => o.context.clone(),
            Place::Street(ref o) => o.context.clone(),
            Place::Addr(ref o) => o.context.clone(),
            Place::Poi(ref o) => o.context.clone(),
            Place::Stop(ref o) => o.context.clone(),
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
    fn admins(&self) -> Vec<Arc<Admin>>;
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

impl<T: MimirObject> MimirObject for Arc<T> {
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

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Default)]
pub struct Property {
    pub key: String,
    pub value: String,
}

impl From<navitia_poi_model::Property> for Property {
    fn from(property: navitia_poi_model::Property) -> Property {
        Property {
            key: property.key,
            value: property.value,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Poi {
    pub id: String,
    pub label: String,
    pub name: String,
    pub coord: Coord,
    /// coord used for some geograhic queries in ES, less precise but  faster than `coord`
    /// https://www.elastic.co/guide/en/elasticsearch/reference/2.4/geo-shape.html
    #[serde(skip_deserializing)]
    pub approx_coord: Option<Geometry>,
    pub administrative_regions: Vec<Arc<Admin>>,
    pub weight: f64,
    pub zip_codes: Vec<String>,
    pub poi_type: PoiType,
    pub properties: Vec<Property>,
    pub address: Option<Address>,
    #[serde(default)]
    pub country_codes: Vec<String>,

    #[serde(default)]
    pub names: I18nProperties,
    #[serde(default)]
    pub labels: I18nProperties,

    /// Distance to the coord in query.
    /// Not serialized as is because it is returned in the `Feature` object
    #[serde(default, skip)]
    pub distance: Option<u32>,

    pub context: Option<Context>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PoiType {
    pub id: String,
    pub name: String,
}

impl From<&navitia_poi_model::PoiType> for PoiType {
    fn from(poi_type: &navitia_poi_model::PoiType) -> PoiType {
        PoiType {
            id: poi_type.id.clone(),
            name: poi_type.name.clone(),
        }
    }
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
    fn admins(&self) -> Vec<Arc<Admin>> {
        self.administrative_regions.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct CommercialMode {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct PhysicalMode {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Network {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Code {
    pub name: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Default)]
pub struct Line {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_color: Option<Rgb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commercial_mode: Option<CommercialMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<Network>,
    pub physical_modes: Vec<PhysicalMode>,
    #[serde(skip_serializing)]
    pub sort_order: Option<u32>, // we do not serialise this field, it is only used to sort the Lines
}

pub trait FromTransitModel<T> {
    fn from_transit_model(idx: Idx<T>, navitia: &transit_model::Model) -> Self;
}

impl FromTransitModel<transit_model::objects::Line> for Line {
    fn from_transit_model(
        l_idx: Idx<transit_model::objects::Line>,
        navitia: &transit_model::Model,
    ) -> Self {
        let line = &navitia.lines[l_idx];
        Self {
            id: normalize_id("line", &line.id),
            name: line.name.clone(),
            code: line.code.clone(),
            color: line.color.clone(),
            sort_order: line.sort_order,
            text_color: line.text_color.clone(),
            commercial_mode: navitia
                .commercial_modes
                .get(&line.commercial_mode_id)
                .map(|c| CommercialMode {
                    id: normalize_id("commercial_mode", &c.id),
                    name: c.name.clone(),
                }),
            network: navitia.networks.get(&line.network_id).map(|n| Network {
                id: normalize_id("network", &n.id),
                name: n.name.clone(),
            }),
            physical_modes: navitia
                .get_corresponding_from_idx(l_idx)
                .into_iter()
                .map(|p_idx| {
                    let physical_mode = &navitia.physical_modes[p_idx];
                    PhysicalMode {
                        id: normalize_id("physical_mode", &physical_mode.id),
                        name: physical_mode.name.clone(),
                    }
                })
                .collect(),
        }
    }
}

#[derive(Default, Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct I18nProperties(pub Vec<Property>);

impl serde::Serialize for I18nProperties {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_map(self.0.iter().map(|p| (&p.key, &p.value)))
    }
}

impl<'de> Deserialize<'de> for I18nProperties {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let properties = BTreeMap::<String, String>::deserialize(deserializer)?
            .into_iter()
            .collect();
        Ok(properties)
    }
}

impl FromIterator<(String, String)> for I18nProperties {
    fn from_iter<I: IntoIterator<Item = (String, String)>>(iter: I) -> Self {
        let properties = iter
            .into_iter()
            .map(|(k, v)| Property { key: k, value: v })
            .collect::<Vec<_>>();
        I18nProperties(properties)
    }
}

impl I18nProperties {
    pub fn get(&self, lang: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|p| p.key == lang)
            .map(|p| p.value.as_ref())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct FeedPublisher {
    pub id: String,
    pub license: String,
    pub name: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Comment {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Stop {
    pub id: String,
    pub label: String,
    pub name: String,
    /// coord used for some geograhic queries in ES, less precise but  faster than `coord`
    /// https://www.elastic.co/guide/en/elasticsearch/reference/2.4/geo-shape.html
    #[serde(skip_deserializing)]
    pub approx_coord: Option<Geometry>,
    pub coord: Coord,
    pub administrative_regions: Vec<Arc<Admin>>,
    pub weight: f64,
    pub zip_codes: Vec<String>,
    #[serde(default)]
    pub commercial_modes: Vec<CommercialMode>,
    #[serde(default)]
    pub physical_modes: Vec<PhysicalMode>,
    #[serde(default)]
    pub coverages: Vec<String>,
    #[serde(default)]
    pub comments: Vec<Comment>,
    #[serde(default)]
    pub timezone: String,
    #[serde(default)]
    pub codes: Vec<Code>,
    #[serde(default)]
    pub properties: Vec<Property>,
    #[serde(default)]
    pub feed_publishers: Vec<FeedPublisher>,
    /// Distance to the coord in query.
    /// Not serialized as is because it is returned in the `Feature` object
    #[serde(default, skip)]
    pub distance: Option<u32>,
    #[serde(default)]
    pub lines: Vec<Line>,
    #[serde(default)]
    pub country_codes: Vec<String>,

    pub context: Option<Context>,
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
    fn admins(&self) -> Vec<Arc<Admin>> {
        self.administrative_regions.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Admin {
    pub id: String,
    pub insee: String,
    pub level: u32,
    pub label: String,
    pub name: String,
    pub zip_codes: Vec<String>,
    pub weight: f64,
    /// coord used for some geograhic queries in ES, less precise but  faster than `coord`
    /// https://www.elastic.co/guide/en/elasticsearch/reference/2.4/geo-shape.html
    #[serde(skip_deserializing)]
    pub approx_coord: Option<Geometry>,
    pub coord: Coord,
    #[serde(
        serialize_with = "custom_multi_polygon_serialize",
        deserialize_with = "custom_multi_polygon_deserialize",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub boundary: Option<MultiPolygon<f64>>,
    #[serde(default)]
    pub administrative_regions: Vec<Arc<Admin>>,

    #[serde(
        serialize_with = "serialize_rect",
        deserialize_with = "deserialize_rect",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub bbox: Option<Rect<f64>>,

    #[serde(default)]
    pub zone_type: Option<ZoneType>,
    #[serde(default)]
    pub parent_id: Option<String>, // id of the Admin's parent (from the cosmogony's hierarchy)
    #[serde(default)]
    pub country_codes: Vec<String>,

    #[serde(default)]
    pub codes: Vec<Code>,

    #[serde(default)]
    pub names: I18nProperties,

    #[serde(default)]
    pub labels: I18nProperties,
    /// Distance to the coord in query.
    /// Not serialized as is because it is returned in the `Feature` object
    #[serde(default, skip)]
    pub distance: Option<u32>,

    pub context: Option<Context>,
}

impl Admin {
    pub fn is_city(&self) -> bool {
        matches!(self.zone_type, Some(ZoneType::City))
    }
}

fn custom_multi_polygon_serialize<S>(
    multi_polygon_option: &Option<MultiPolygon<f64>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    use geojson::{GeoJson, Value};

    match *multi_polygon_option {
        Some(ref multi_polygon) => {
            GeoJson::Geometry(Geometry::new(Value::from(multi_polygon))).serialize(serializer)
        }
        None => serializer.serialize_none(),
    }
}

fn custom_multi_polygon_deserialize<'de, D>(d: D) -> Result<Option<MultiPolygon<f64>>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    use std::convert::TryInto;

    Option::<geojson::GeoJson>::deserialize(d).map(|option| {
        option.and_then(|geojson| match geojson {
            geojson::GeoJson::Geometry(geojson_geometry) => {
                let res: Result<MultiPolygon<f64>, _> = geojson_geometry.value.try_into();
                match res {
                    Ok(multi_polygon) => Some(multi_polygon),
                    Err(err) => {
                        warn!("Cannot deserialize into MultiPolygon: {}", err);
                        None
                    }
                }
            }
            _ => None,
        })
    })
}

pub fn serialize_rect<S>(bbox: &Option<Rect<f64>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match bbox {
        Some(b) => {
            // bbox serialized as an array
            // using GeoJSON bounding box format
            // See RFC 7946: https://tools.ietf.org/html/rfc7946#section-5
            let geojson_bbox: geojson::Bbox = vec![b.min().x, b.min().y, b.max().x, b.max().y];
            geojson_bbox.serialize(serializer)
        }
        None => serializer.serialize_none(),
    }
}

fn deserialize_rect<'de, D>(d: D) -> Result<Option<Rect<f64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Vec<f64>>::deserialize(d).map(|option| {
        option.map(|b| {
            Rect::new(
                Coordinate { x: b[0], y: b[1] }, // min
                Coordinate { x: b[2], y: b[3] }, // max
            )
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
    fn admins(&self) -> Vec<Arc<Admin>> {
        vec![Arc::new(self.clone())]
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
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Street {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub administrative_regions: Vec<Arc<Admin>>,
    pub label: String,
    pub weight: f64,
    /// coord used for some geograhic queries in ES, less precise but  faster than `coord`
    /// https://www.elastic.co/guide/en/elasticsearch/reference/2.4/geo-shape.html
    #[serde(skip_deserializing)]
    pub approx_coord: Option<Geometry>,
    pub coord: Coord,
    pub zip_codes: Vec<String>,
    #[serde(default)]
    pub country_codes: Vec<String>,
    /// Distance to the coord in query.
    /// Not serialized as is because it is returned in the `Feature` object
    #[serde(default, skip)]
    pub distance: Option<u32>,

    pub context: Option<Context>,
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
    fn admins(&self) -> Vec<Arc<Admin>> {
        self.administrative_regions.clone()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Addr {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub house_number: String,
    pub street: Street,
    pub label: String,
    pub coord: Coord,
    /// coord used for some geograhic queries in ES, less precise but  faster than `coord`
    /// https://www.elastic.co/guide/en/elasticsearch/reference/2.4/geo-shape.html
    #[serde(skip_deserializing)]
    pub approx_coord: Option<Geometry>,
    pub weight: f64,
    pub zip_codes: Vec<String>,
    #[serde(default)]
    pub country_codes: Vec<String>,
    /// Distance to the coord in query.
    /// Not serialized as is because it is returned in the `Feature` object
    #[serde(default, skip)]
    pub distance: Option<u32>,

    pub context: Option<Context>,
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
    fn admins(&self) -> Vec<Arc<Admin>> {
        self.street.admins()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AliasOperations {
    pub actions: Vec<AliasOperation>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AliasOperation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add: Option<AliasParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove: Option<AliasParameter>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AliasParameter {
    pub index: String,
    pub alias: String,
}

// we want a custom serialization for coords, and so far the cleanest way
// to do this that has been found is to wrap the coord in another struct
#[derive(Debug, Clone, Copy)]
pub struct Coord(pub geo_types::Coordinate<f64>);

impl Coord {
    pub fn new(lon: f64, lat: f64) -> Coord {
        Coord(geo_types::Coordinate { x: lon, y: lat })
    }
    pub fn lon(&self) -> f64 {
        self.x
    }
    pub fn lat(&self) -> f64 {
        self.y
    }
    pub fn is_default(&self) -> bool {
        self.lat() == 0. && self.lon() == 0.
    }
    pub fn is_valid(&self) -> bool {
        !self.is_default()
            && -90. <= self.lat()
            && self.lat() <= 90.
            && -180. <= self.lon()
            && self.lon() <= 180.
    }
}

impl Default for Coord {
    fn default() -> Coord {
        Coord(geo_types::Coordinate { x: 0., y: 0. })
    }
}

impl ::std::ops::Deref for Coord {
    type Target = geo_types::Coordinate<f64>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl serde::Serialize for Coord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut ser = serializer.serialize_struct("Coord", 2)?;
        ser.serialize_field("lon", &self.0.x)?;
        ser.serialize_field("lat", &self.0.y)?;
        ser.end()
    }
}

impl From<Coord> for Geometry {
    fn from(coord: Coord) -> Geometry {
        Geometry::new(geojson::Value::Point(vec![coord.lon(), coord.lat()]))
    }
}

impl<'de> Deserialize<'de> for Coord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Lon,
            Lat,
        };

        struct CoordVisitor;

        impl<'de> Visitor<'de> for CoordVisitor {
            type Value = Coord;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("struct Coord")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<Coord, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let lon = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let lat = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(Coord::new(lon, lat))
            }

            fn visit_map<V>(self, mut map: V) -> Result<Coord, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut lat = None;
                let mut lon = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Lat => {
                            if lat.is_some() {
                                return Err(de::Error::duplicate_field("lat"));
                            }
                            lat = Some(map.next_value()?);
                        }
                        Field::Lon => {
                            if lon.is_some() {
                                return Err(de::Error::duplicate_field("lon"));
                            }
                            lon = Some(map.next_value()?);
                        }
                    }
                }
                let lat = lat.ok_or_else(|| de::Error::missing_field("lat"))?;
                let lon = lon.ok_or_else(|| de::Error::missing_field("lon"))?;
                Ok(Coord::new(lon, lat))
            }
        }

        const FIELDS: &[&str] = &["lat", "lon"];
        deserializer.deserialize_struct("Coord", FIELDS, CoordVisitor)
    }
}

impl From<&navitia_poi_model::Coord> for Coord {
    fn from(coord: &navitia_poi_model::Coord) -> Coord {
        Coord::new(coord.lon(), coord.lat())
    }
}

/// Contextual information related to the query. It can be used to store information
/// for monitoring performance, search relevance, ...
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Context {
    /// Elasticsearch explanation
    pub explanation: Option<Explanation>,
}

/// This structure is used when analyzing the result of an Elasticsearch 'explanation' query,
/// which describes the construction of the score". It is a tree structure.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Explanation {
    /// score assigned by elasticsearch for that item
    pub value: f64,
    /// description of the operation used to obtained `value` from each `details` values.
    pub description: String,
    /// leafs
    pub details: Vec<Explanation>,
}

// This function reformat the id by removing spaces, and prepending a prefix
pub fn normalize_id(prefix: &str, id: &str) -> String {
    match prefix {
        "stop_area" => format!(
            "{}:{}",
            prefix,
            &id.replacen("StopArea:", "", 1).replace(" ", "")
        ),
        _ => format!("{}:{}", prefix, &id.replace(" ", "")),
    }
}

#[test]
fn test_normalize_id() {
    assert_eq!(
        normalize_id("stop_area", "an id with space"),
        "stop_area:anidwithspace"
    );
    assert_eq!(
        normalize_id("stop_area", "SIN:SA:ABCDE:StopArea:1234"),
        "stop_area:SIN:SA:ABCDE:1234"
    );
}
