// Copyright © 2018, Canal TP and/or its affiliates. All rights reserved.
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

extern crate csv;
extern crate failure;
extern crate geo;
extern crate mimir;
extern crate mimirsbrunn;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate slog;
#[macro_use]
extern crate slog_scope;
#[macro_use]
extern crate structopt;

use failure::ResultExt;
use mimir::rubber::Rubber;
use mimirsbrunn::admin_geofinder::AdminGeoFinder;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct OpenAddresse {
    pub id: String,
    pub street: String,
    pub postcode: String,
    pub district: String,
    pub region: String,
    pub city: String,
    pub number: String,
    pub unit: String,
    pub lat: f64,
    pub lon: f64,
}

impl OpenAddresse {
    pub fn into_addr(self, admins_geofinder: &AdminGeoFinder) -> mimir::Addr {
        let street_name = format!("{} ({})", self.street, self.city);
        let addr_name = format!("{} {}", self.number, self.street);
        let addr_label = format!("{} ({})", addr_name, self.city);
        let street_id = format!("street:{}", self.id); // TODO check if thats ok
        let admins = admins_geofinder.get(&geo::Coordinate {
            x: self.lon,
            y: self.lat,
        });

        let weight = admins
            .iter()
            .find(|a| a.is_city())
            .map_or(0., |a| a.weight.get());

        let street = mimir::Street {
            id: street_id,
            street_name: self.street.clone(),
            name: self.street,
            label: street_name.to_string(),
            administrative_regions: admins,
            weight: weight,
            zip_codes: vec![self.postcode.clone()],
            coord: mimir::Coord::new(self.lon, self.lat),
        };
        mimir::Addr {
            id: format!("addr:{};{}", self.lon, self.lat),
            house_number: self.number,
            street: street,
            label: addr_label,
            coord: mimir::Coord::new(self.lon, self.lat),
            weight: weight,
            zip_codes: vec![self.postcode.clone()],
        }
    }
}

fn index_oa<I>(cnx_string: &str, dataset: &str, files: I) -> Result<(), mimirsbrunn::Error>
where
    I: Iterator<Item = std::path::PathBuf>,
{
    let mut rubber = Rubber::new(cnx_string);

    let admins = rubber
        .get_admins_from_dataset(dataset)
        .unwrap_or_else(|err| {
            info!(
                "Administratives regions not found in es db for dataset {}. (error: {})",
                dataset, err
            );
            vec![]
        });
    let admins_geofinder = admins.into_iter().collect();

    let addr_index = rubber
        .make_index(dataset)
        .with_context(|_| format!("error occureed when making index for {}", dataset))?;
    info!("Add data in elasticsearch db.");
    for f in files {
        info!("importing {:?}...", &f);
        let mut rdr = csv::ReaderBuilder::new().has_headers(true).from_path(&f)?;
        let iter = rdr.deserialize().filter_map(|r| {
            r.map_err(|e| info!("impossible to read line, error: {}", e))
                .ok()
                .map(|v: OpenAddresse| v.into_addr(&admins_geofinder))
        });
        let nb = rubber
            .bulk_index(&addr_index, iter)
            .with_context(|_| format!("failed to bulk insert file {:?}", &f))?;
        info!("importing {:?}: {} addresses added.", &f, nb);
    }
    rubber.publish_index(dataset, addr_index)
}

#[derive(StructOpt, Debug)]
struct Args {
    /// openaddresses files. Can be either a directory or a file.
    #[structopt(short = "i", long = "input", parse(from_os_str))]
    input: PathBuf,
    /// Elasticsearch parameters.
    #[structopt(
        short = "c", long = "connection-string", default_value = "http://localhost:9200/munin"
    )]
    connection_string: String,
    /// Name of the dataset.
    #[structopt(short = "d", long = "dataset", default_value = "fr")]
    dataset: String,
    /// Deprecated option.
    #[structopt(short = "C", long = "city-level")]
    city_level: Option<String>,
}

fn run(args: Args) -> Result<(), failure::Error> {
    info!("importing open addresses into Mimir");

    if args.city_level.is_some() {
        warn!("city-level option is deprecated, it now has no effect.");
    }

    if args.input.is_dir() {
        let paths: std::fs::ReadDir = fs::read_dir(&args.input)?;
        index_oa(
            &args.connection_string,
            &args.dataset,
            paths.map(|p| p.unwrap().path()),
        )
    } else {
        index_oa(
            &args.connection_string,
            &args.dataset,
            std::iter::once(args.input),
        )
    }
}

fn main() {
    mimirsbrunn::utils::launch_run(run);
}
