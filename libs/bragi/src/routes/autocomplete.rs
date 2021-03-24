use crate::extractors::BragiQuery;
use crate::model::{Autocomplete, BragiError, FromWithLang};
use crate::routes::params;
use crate::{model, query, Context};
use actix_http::http::header::{CacheControl, CacheDirective};
use actix_web::web::{Data, HttpResponse, Json};
use geojson::{GeoJson, Geometry};
use mimir::objects::Coord;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
enum Type {
    #[serde(rename = "city")]
    City,
    #[serde(rename = "house")]
    House,
    #[serde(rename = "poi")]
    Poi,
    #[serde(rename = "public_transport:stop_area")]
    StopArea,
    #[serde(rename = "street")]
    Street,
    #[serde(rename = "zone")]
    Zone,
}

impl Type {
    fn as_str(self) -> &'static str {
        match self {
            Type::City => "city",
            Type::House => "house",
            Type::Poi => "poi",
            Type::StopArea => "public_transport:stop_area",
            Type::Street => "street",
            Type::Zone => "zone",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum PoiType {
    Whatever(String),
}

impl PoiType {
    fn as_str(&self) -> &str {
        match *self {
            PoiType::Whatever(ref s) => s,
        }
    }
}

fn default_limit() -> u64 {
    10u64
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Params {
    q: String,
    #[serde(default)]
    pt_dataset: Vec<String>,
    #[serde(default)]
    poi_dataset: Vec<String>,
    #[serde(rename = "_all_data", default)]
    all_data: bool,
    //Note: for the moment we can't use an external struct and flatten it (https://github.com/nox/serde_urlencoded/issues/33)
    #[serde(default = "default_limit")]
    limit: u64,
    #[serde(default)]
    offset: u64,
    /// timeout in milliseconds
    timeout: Option<u64>,
    // Position of the request
    lat: Option<f64>,
    lon: Option<f64>,
    // If specified, override parameters for the normal decay computed by elasticsearch around the
    // position: https://www.elastic.co/guide/en/elasticsearch/reference/current/query-dsl-function-score-query.html#_supported_decay_functions
    proximity_scale: Option<f64>,
    proximity_offset: Option<f64>,
    proximity_decay: Option<f64>,
    #[serde(default, rename = "type")]
    types: Vec<Type>,
    #[serde(default, rename = "zone_type")]
    zone_types: Vec<cosmogony::ZoneType>,
    #[serde(default, rename = "poi_type")]
    poi_types: Vec<PoiType>,
    lang: Option<String>,
    // The scope is a list of place types on which we apply the shape filter.
    // Places found in this list are restricted to the shape.
    #[serde(default)]
    shape_scope: Vec<String>,
    // Forwards a request for explanation to Elastic Search.
    // This parameter is useful to analyze the order in which search results appear.
    // It is prefixed by an underscore to indicate its not a public parameter.
    #[serde(default, rename = "_debug")]
    debug: Option<bool>,
}

impl Params {
    fn types_as_str(&self) -> Vec<&str> {
        self.types.iter().map(|t| Type::as_str(*t)).collect()
    }
    fn zone_types_as_str(&self) -> Vec<&str> {
        self.zone_types.iter().map(|x| x.as_str()).collect()
    }
    fn poi_types_as_str(&self) -> Vec<&str> {
        self.poi_types.iter().map(PoiType::as_str).collect()
    }
    fn coord(&self) -> Result<Option<Coord>, BragiError> {
        Self::build_coord(self.lon, self.lat)
    }
    fn langs(&self) -> Vec<&str> {
        self.lang.iter().map(|l| l.as_str()).collect()
    }
    fn timeout(&self) -> Option<Duration> {
        self.timeout.map(Duration::from_millis)
    }
    fn build_coord(lon: Option<f64>, lat: Option<f64>) -> Result<Option<Coord>, BragiError> {
        match (lon, lat) {
            (Some(lon), Some(lat)) => Ok(Some(params::make_coord(lon, lat)?)),
            (None, None) => Ok(None),
            _ => Err(BragiError::InvalidParam(
                "you should provide a 'lon' AND a 'lat' parameter if you provide one of them",
            )),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsonParams {
    shape: GeoJson,
}

impl JsonParams {
    fn get_geometry(self) -> Result<Geometry, model::BragiError> {
        match self.shape {
            GeoJson::Feature(f) => f.geometry.ok_or(BragiError::InvalidShape("no geometry")),
            _ => Err(BragiError::InvalidShape("only 'feature' is supported")),
        }
    }
}

pub fn call_autocomplete(
    params: &Params,
    state: &Context,
    shape: Option<Geometry>,
) -> Result<HttpResponse, model::BragiError> {
    let langs = params.langs();
    let rubber = state.get_rubber_for_autocomplete(params.timeout());
    let mut query_settings = state.get_query_settings().clone();

    if let Some(scale) = params.proximity_scale {
        query_settings.importance_query.proximity.gaussian.scale = scale;
    }

    if let Some(offset) = params.proximity_offset {
        query_settings.importance_query.proximity.gaussian.offset = offset;
    }

    if let Some(decay) = params.proximity_decay {
        query_settings.importance_query.proximity.gaussian.decay = decay;
    }

    let res = query::autocomplete(
        &params.q,
        &params
            .pt_dataset
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        &params
            .poi_dataset
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        params.all_data,
        params.offset,
        params.limit,
        params.coord()?,
        shape,
        &params
            .shape_scope
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        &params.types_as_str(),
        &params.zone_types_as_str(),
        &params.poi_types_as_str(),
        &langs,
        rubber,
        params.debug.unwrap_or(false),
        &query_settings,
    );
    res.map(|r| Autocomplete::from_with_lang(r, langs.into_iter().next()))
        .map(|v| {
            HttpResponse::Ok()
                .set(CacheControl(vec![CacheDirective::MaxAge(
                    state.http_cache_duration,
                )]))
                .json(v)
        })
}

pub fn autocomplete(
    params: BragiQuery<Params>,
    state: Data<Context>,
) -> Result<HttpResponse, model::BragiError> {
    call_autocomplete(&*params, &*state, None)
}

pub fn post_autocomplete(
    params: BragiQuery<Params>,
    state: Data<Context>,
    json_params: Json<JsonParams>,
) -> Result<HttpResponse, model::BragiError> {
    call_autocomplete(
        &*params,
        &*state,
        Some(json_params.into_inner().get_geometry()?),
    )
}
