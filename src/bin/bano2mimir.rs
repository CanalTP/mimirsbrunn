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

use failure::ensure;
use lazy_static::lazy_static;
use mimir::objects::Admin;
use mimir::rubber::{IndexSettings, Rubber};
use mimirsbrunn::addr_reader::import_addresses;
use mimirsbrunn::admin_geofinder::AdminGeoFinder;
use mimirsbrunn::labels;
use serde::{Deserialize, Serialize};
use slog_scope::{info, warn};
use std::collections::BTreeMap;
use std::fs;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use structopt::StructOpt;

type AdminFromInsee = BTreeMap<String, Arc<Admin>>;

lazy_static! {
    static ref DEFAULT_NB_THREADS: String = num_cpus::get().to_string();
}

#[derive(Serialize, Deserialize)]
pub struct Bano {
    pub id: String,
    pub nb: String,
    pub street: String,
    pub zip: String,
    pub city: String,
    pub src: String,
    pub lat: f64,
    pub lon: f64,
}

impl Bano {
    pub fn insee(&self) -> Result<&str, mimirsbrunn::Error> {
        ensure!(self.id.len() >= 5, "id must be longer than 5 characters");
        Ok(self.id[..5].trim_start_matches('0'))
    }
    pub fn fantoir(&self) -> Result<&str, mimirsbrunn::Error> {
        ensure!(self.id.len() >= 10, "id must be longer than 10 characters");
        Ok(&self.id[..10])
    }
    pub fn into_addr(
        self,
        admins_from_insee: &AdminFromInsee,
        admins_geofinder: &AdminGeoFinder,
        use_old_index_format: bool,
    ) -> Result<mimir::Addr, mimirsbrunn::Error> {
        let street_id = format!("street:{}", self.fantoir()?.to_string());
        let mut admins = admins_geofinder.get(&geo::Coordinate {
            x: self.lon,
            y: self.lat,
        });

        // If we have an admin corresponding to the INSEE, we know
        // that's the good one, thus we remove all the admins of its
        // level found by the geofinder, and add our admin.
        if let Some(admin) = admins_from_insee.get(self.insee()?) {
            admins.retain(|a| a.level != admin.level);
            admins.push(admin.clone());
        }

        let country_codes = vec!["fr".to_owned()];

        // to format the label of the addr/street, we use bano's city
        // even if we already have found a city in the admin_geo_finder
        let city = build_admin_from_bano_city(&self.city);
        let zones_for_label_formatting = admins
            .iter()
            .filter(|a| a.is_city())
            .map(|a| a.deref())
            .chain(std::iter::once(&city));

        let street_label = labels::format_street_label(
            &self.street,
            zones_for_label_formatting.clone(),
            &country_codes,
        );
        let (addr_name, addr_label) = labels::format_addr_name_and_label(
            &self.nb,
            &self.street,
            zones_for_label_formatting,
            &country_codes,
        );

        let weight = admins
            .iter()
            .find(|a| a.level == 8)
            .map_or(0., |a| a.weight);

        let coord = mimir::Coord::new(self.lon, self.lat);
        let street = mimir::Street {
            id: street_id,
            name: self.street,
            label: street_label,
            administrative_regions: admins,
            weight: weight,
            zip_codes: vec![self.zip.clone()],
            coord: coord.clone(),
            approx_coord: None,
            distance: None,
            country_codes: country_codes.clone(),
            context: None,
        };
        Ok(mimir::Addr {
            id: format!(
                "addr:{};{}{}",
                self.lon,
                self.lat,
                if use_old_index_format {
                    String::new()
                } else {
                    format!(
                        ":{}",
                        self.nb
                            .replace(" ", "")
                            .replace("\t", "")
                            .replace("\r", "")
                            .replace("\n", "")
                            .replace("/", "-")
                            .replace(".", "-")
                            .replace(":", "-")
                            .replace(";", "-")
                    )
                }
            ),
            name: addr_name,
            label: addr_label,
            house_number: self.nb,
            street: street,
            coord: coord.clone(),
            approx_coord: Some(coord.into()),
            weight: weight,
            zip_codes: vec![self.zip.clone()],
            distance: None,
            country_codes,
            context: None,
        })
    }
}

fn build_admin_from_bano_city(city: &str) -> Admin {
    mimir::Admin {
        name: city.to_string(),
        zone_type: Some(cosmogony::ZoneType::City),
        ..Default::default()
    }
}

fn index_bano<I>(
    cnx_string: &str,
    dataset: &str,
    files: I,
    nb_threads: usize,
    nb_shards: usize,
    nb_replicas: usize,
    use_old_index_format: bool,
) -> Result<(), mimirsbrunn::Error>
where
    I: Iterator<Item = std::path::PathBuf>,
{
    let mut rubber = Rubber::new(cnx_string);
    rubber.initialize_templates()?;

    let admins = rubber.get_all_admins().unwrap_or_else(|err| {
        warn!(
            "Administratives regions not found in es db for dataset {}. (error: {})",
            dataset, err
        );
        vec![]
    });
    let admins_geofinder = admins.iter().cloned().collect();
    let admins_by_insee = admins
        .into_iter()
        .filter(|a| !a.insee.is_empty())
        .map(|mut a| {
            a.boundary = None; // to save some space we remove the admin boundary
            (a.insee.clone(), Arc::new(a))
        })
        .collect();

    let index_settings = IndexSettings {
        nb_shards: nb_shards,
        nb_replicas: nb_replicas,
    };

    import_addresses(
        &mut rubber,
        false,
        nb_threads,
        index_settings,
        dataset,
        files,
        move |b: Bano| b.into_addr(&admins_by_insee, &admins_geofinder, use_old_index_format),
    )
}

#[derive(StructOpt, Debug)]
struct Args {
    /// Bano files. Can be either a directory or a file.
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
    #[structopt(short = "d", long = "dataset", default_value = "fr")]
    dataset: String,
    /// Number of threads to use
    #[structopt(
        short = "t",
        long = "nb-threads",
        default_value = &DEFAULT_NB_THREADS
    )]
    nb_threads: usize,
    /// Number of shards for the es index
    #[structopt(short = "s", long = "nb-shards", default_value = "5")]
    nb_shards: usize,
    /// Number of replicas for the es index
    #[structopt(short = "r", long = "nb-replicas", default_value = "1")]
    nb_replicas: usize,
    /// If set to true, the number inside the address won't be used for the index generation,
    /// therefore, different addresses with the same position will disappear.
    #[structopt(long = "use-old-index-format")]
    use_old_index_format: bool,
}

fn run(args: Args) -> Result<(), mimirsbrunn::Error> {
    info!("importing bano into Mimir");
    if args.input.is_dir() {
        let paths: std::fs::ReadDir = fs::read_dir(&args.input)?;
        index_bano(
            &args.connection_string,
            &args.dataset,
            paths.map(|p| p.expect("failed to unwrap path").path()),
            args.nb_threads,
            args.nb_shards,
            args.nb_replicas,
            args.use_old_index_format,
        )
    } else {
        index_bano(
            &args.connection_string,
            &args.dataset,
            std::iter::once(args.input),
            args.nb_threads,
            args.nb_shards,
            args.nb_replicas,
            args.use_old_index_format,
        )
    }
}
fn main() {
    mimirsbrunn::utils::launch_run(run);
}
