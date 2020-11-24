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

use failure::ResultExt;
use mimir::rubber::{IndexSettings, Rubber};
use mimirsbrunn::admin_geofinder::AdminGeoFinder;
use mimirsbrunn::osm_reader::admin::read_administrative_regions;
use mimirsbrunn::osm_reader::make_osm_reader;
use mimirsbrunn::osm_reader::poi::{add_address, compute_poi_weight, pois, PoiConfig};
use mimirsbrunn::osm_reader::street::{compute_street_weight, streets};
use mimirsbrunn::settings::osm2mimir::{Args, Settings};
use slog_scope::{debug, info};

fn run(args: Args) -> Result<(), mimirsbrunn::Error> {
    let input = args.input.clone();
    validate_args(&args)?;
    let settings = Settings::new(args)?;

    println!("{:#?}", settings);
    // return Ok(());

    let mut osm_reader = make_osm_reader(&input)?;
    debug!("creation of indexes");
    let mut rubber = Rubber::new(&settings.elasticsearch.connection_string)
        .with_nb_insert_threads(settings.elasticsearch.insert_thread_count);
    rubber.initialize_templates()?;

    let settings = &settings;
    info!("creating administrative regions");
    let admins = if settings
        .admin
        .as_ref()
        .map(|admin| admin.import)
        .unwrap_or_else(|| false)
    {
        // If we want to import admins, then the admin section should be present
        if settings.admin.is_none() {
            return Err(failure::format_err!("You need to specify admin settings, either through configuration file or command line arguments."));
        }
        let admins = settings.admin.as_ref().unwrap();
        let levels = admins.levels.iter().cloned().collect();
        let city_level = admins.city_level;
        read_administrative_regions(&mut osm_reader, levels, city_level)
    } else {
        rubber.get_all_admins()?
    };

    let admins_geofinder = admins.into_iter().collect::<AdminGeoFinder>();

    if settings
        .way
        .as_ref()
        .map(|way| way.import)
        .unwrap_or_else(|| false)
    {
        info!("Extracting streets from osm");
        let mut streets = streets(&mut osm_reader, &admins_geofinder, &settings)?;

        info!("computing street weight");
        compute_street_weight(&mut streets);

        let street_index_settings = IndexSettings {
            nb_shards: settings.elasticsearch.streets_shards,
            nb_replicas: settings.elasticsearch.streets_replicas,
        };
        info!("importing streets into Mimir");
        let nb_streets = rubber
            .public_index(
                &settings.dataset,
                &street_index_settings,
                streets.into_iter(),
            )
            .with_context(|err| {
                format!(
                    "Error occurred when requesting street number in {}: {}",
                    settings.dataset, err
                )
            })?;
        info!("Nb of indexed street: {}", nb_streets);
    }
    if settings
        .admin
        .as_ref()
        .map(|admin| admin.import)
        .unwrap_or_else(|| false)
    {
        let admin_index_settings = IndexSettings {
            nb_shards: settings.elasticsearch.admins_shards,
            nb_replicas: settings.elasticsearch.admins_replicas,
        };
        let nb_admins = rubber
            .public_index(
                &settings.dataset,
                &admin_index_settings,
                admins_geofinder.admins(),
            )
            .with_context(|err| {
                format!(
                    "Error occurred when requesting admin number in {}: {}",
                    settings.dataset, err
                )
            })?;
        info!("Nb of indexed admin: {}", nb_admins);
    }

    if settings
        .poi
        .as_ref()
        .map(|poi| poi.import)
        .unwrap_or_else(|| false)
    {
        let config = settings
            .poi
            .as_ref()
            .and_then(|poi| poi.config.clone())
            .unwrap_or_else(PoiConfig::default);

        info!("Extracting pois from osm");
        let mut pois = pois(&mut osm_reader, &config, &admins_geofinder);

        info!("computing poi weight");
        compute_poi_weight(&mut pois);

        info!("Adding address in poi");
        add_address(&mut pois, &mut rubber);

        let poi_index_settings = IndexSettings {
            nb_shards: settings.elasticsearch.pois_shards,
            nb_replicas: settings.elasticsearch.pois_replicas,
        };
        info!("Importing pois into Mimir");
        let nb_pois = rubber
            .public_index(&settings.dataset, &poi_index_settings, pois.into_iter())
            .context("Importing pois into Mimir")?;

        info!("Nb of indexed pois: {}", nb_pois);
    }
    Ok(())
}

// We need to allow for unused variables, because currently all the checks on
// args require the db-storage feature. If this feature is not used, then there
// is a warning
#[allow(unused_variables)]
fn validate_args(args: &Args) -> Result<(), mimirsbrunn::Error> {
    #[cfg(feature = "db-storage")]
    if args.db_file.is_some() {
        // If the user specified db_file, he must also specify db_buffer_size, or else!
        if args.db_buffer_size.is_none() {
            return Err(failure::format_err!("You need to specify database buffer size if you want to use database storage. Use --db-buffer-size"));
        }
    }
    #[cfg(feature = "db-storage")]
    if args.db_buffer_size.is_some() {
        // If the user specified db_buffer_size, he must also specify db_file, or else!
        if args.db_file.is_none() {
            return Err(failure::format_err!("You need to specify database file if you want to use database storage. Use --db-file"));
        }
    }
    Ok(())
}

fn main() {
    mimirsbrunn::utils::launch_run(run);
}
