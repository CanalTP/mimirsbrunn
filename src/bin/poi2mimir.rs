// Copyright © 2019, Canal TP and/or its affiliates. All rights reserved.
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

use slog_scope::{error, info};

use failure::format_err;
use lazy_static::lazy_static;
use mimir::objects::{Coord, I18nProperties, Poi, PoiType, Property};
use mimir::rubber::{IndexSettings, IndexVisibility, Rubber, TypedIndex};
use mimirsbrunn::{admin_geofinder::AdminGeoFinder, labels, utils};
use navitia_poi_model::{Model as NavitiaModel, Poi as NavitiaPoi, PoiType as NavitiaPoiType};
use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

lazy_static! {
    static ref DEFAULT_NB_THREADS: String = num_cpus::get().to_string();
}

// This function takes a Poi from the navitia model, ie from the CSV deserialization, and returns
// a Poi from the mimir model, with all the contextual information added.
fn into_mimir_poi(
    poi: NavitiaPoi,
    poi_types: &HashMap<String, NavitiaPoiType>,
    rubber: &mut Rubber,
    admins_geofinder: &AdminGeoFinder,
) -> Result<Poi, mimirsbrunn::Error> {
    let poi_type = poi_types
        .get(&poi.poi_type_id)
        .ok_or_else(|| format_err!("could not find Poi Type '{}'", poi.poi_type_id))
        .map(PoiType::from)?;

    let coord = Coord::from(&poi.coord);

    let place = rubber
        .get_address(&coord) // No timeout
        .ok()
        .and_then(|addrs| addrs.into_iter().next()); // Take the first place

    let addr = place.as_ref().and_then(|place| place.address());

    // We the the admins from the address, or, if we don't have any, from the geofinder.
    let admins = place.map_or_else(|| admins_geofinder.get(&poi.coord), |addr| addr.admins());

    if admins.is_empty() {
        return Err(format_err!("Could not find admins for POI {}", &poi.id));
    }

    // The weight is that of the city, or 0.0 if there is no such admin.
    let weight: f64 = admins
        .iter()
        .filter(|adm| adm.is_city())
        .map(|adm| adm.weight)
        .next()
        .unwrap_or(0.0);

    let country_codes = utils::find_country_codes(admins.iter().map(|a| a.deref()));

    let label =
        labels::format_poi_label(&poi.name, admins.iter().map(|a| a.deref()), &country_codes);

    let poi = Poi {
        id: mimir::objects::normalize_id("poi", &poi.id),
        label,
        name: poi.name,
        coord,
        approx_coord: Some(coord.into()),
        administrative_regions: admins,
        weight,
        zip_codes: vec![],
        poi_type,
        properties: poi.properties.into_iter().map(Property::from).collect(),
        address: addr,
        country_codes,
        names: I18nProperties::default(),
        labels: I18nProperties::default(),
        distance: None,
        context: None,
    };

    Ok(poi)
}

fn import_pois(
    rubber: &mut Rubber,
    index: &TypedIndex<Poi>,
    admins_geofinder: AdminGeoFinder,
    file: &Path,
) -> Result<(), mimirsbrunn::Error>
where
{
    info!("Add data in elasticsearch db.");

    let model = NavitiaModel::try_from_path(file)?;
    let poi_types = model.poi_types;

    // Note: We're ignoring those POIs that fail to be enriched.
    let pois: Vec<_> = model
        .pois
        .into_iter()
        .filter_map(|(id, poi)| {
            into_mimir_poi(poi, &poi_types, rubber, &admins_geofinder)
                .map_err(|err| info!("Could not extract information for POI '{}': {}", id, err))
                .ok()
        })
        .collect(); // TODO Can we get rid of collect, and chain with the following rubber...?

    let count = rubber
        .bulk_index(&index, pois.into_iter())
        .map_err(|err| format_err!("Failed bulk insertion {}", err))?;

    info!("importing POIs: {} POIs added.", count);

    Ok(())
}

/// This function initializes the ES context: It creates an index for this dataset,
/// and then import the POIs in it.
fn index_poi(
    cnx_string: &str,
    dataset: &str,
    file: &Path,
    visibility: IndexVisibility,
    nb_shards: usize,
    nb_replicas: usize,
) -> Result<(), mimirsbrunn::Error>
where
{
    let mut rubber = Rubber::new(cnx_string);
    rubber.initialize_templates()?;

    let settings = IndexSettings {
        nb_shards,
        nb_replicas,
    };

    let index = rubber.make_index(dataset, &settings)?;

    let admins = rubber.get_all_admins().map_err(|err| {
        error!("Administratives regions not found in es db");
        err
    })?;
    let admins_geofinder = admins.into_iter().collect();

    import_pois(&mut rubber, &index, admins_geofinder, file)?;

    rubber
        .publish_index(dataset, index, visibility)
        .map_err(|err| format_err!("Failed to publish index {}.", err))
}

#[derive(StructOpt, Debug)]
struct Args {
    /// POI file
    #[structopt(short = "i", long = "input", parse(from_os_str))]
    input: PathBuf,

    /// Elasticsearch parameters.
    #[structopt(
        short = "c",
        long = "connection-string",
        default_value = "http://localhost:9200/munin"
    )]
    connection_string: String,

    /// Name of the dataset.
    /// A dataset is a label, that can be used for filtering the data.
    #[structopt(short = "d", long = "dataset", default_value = "fr")]
    dataset: String,

    /// Indicate if the POI dataset is private
    #[structopt(short = "p", long = "private")]
    private: bool,

    /// Number of threads to use
    #[structopt(
        short = "t",
        long = "nb-threads",
        default_value = &DEFAULT_NB_THREADS
    )]
    nb_threads: usize,

    /// Number of shards for the es index
    #[structopt(short = "s", long = "nb-shards", default_value = "1")]
    nb_shards: usize,

    /// Number of replicas for the es index
    #[structopt(short = "r", long = "nb-replicas", default_value = "1")]
    nb_replicas: usize,
}

fn run(args: Args) -> Result<(), mimirsbrunn::Error> {
    let visibility = if args.private {
        IndexVisibility::Private
    } else {
        IndexVisibility::Public
    };

    index_poi(
        &args.connection_string,
        &args.dataset,
        &args.input,
        visibility,
        args.nb_shards,
        args.nb_replicas,
    )
}
fn main() {
    mimirsbrunn::utils::launch_run(run);
}
