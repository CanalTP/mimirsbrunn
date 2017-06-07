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

extern crate docopt;
#[macro_use]
extern crate log;
extern crate mimir;
extern crate mimirsbrunn;
extern crate rustc_serialize;

use mimir::rubber::Rubber;
use mimirsbrunn::osm_reader::admin::{administrative_regions, compute_admin_weight};
use mimirsbrunn::osm_reader::poi::{pois, compute_poi_weight, PoiConfig};
use mimirsbrunn::osm_reader::street::{streets, compute_street_weight};
use mimirsbrunn::osm_reader::parse_osm_pbf;
use mimirsbrunn::admin_geofinder::AdminGeoFinder;


#[derive(RustcDecodable, Debug)]
struct Args {
    flag_input: String,
    flag_level: Vec<u32>,
    flag_city_level: u32,
    flag_connection_string: String,
    flag_import_way: bool,
    flag_import_admin: bool,
    flag_import_poi: bool,
    flag_dataset: String,
    flag_poi_config: Option<String>,
}

static USAGE: &'static str = r#"Usage:
    osm2mimir --help
    osm2mimir [options] --input=<file> --level=<level> ...

Options:
    -h, --help                Show this message.
    -i, --input=<file>        OSM PBF file.
    -l, --level=<level>       Admin levels to keep.
    -C, --city-level=<level>  City level to  calculate weight, [default: 8]
    -w, --import-way          Import ways
    -a, --import-admin        Import admins
    -p, --import-poi          Import POIs
    -c, --connection-string=<connection-string>
                              Elasticsearch parameters, [default: http://localhost:9200/munin]
    -d, --dataset=<dataset>   Name of the dataset, [default: fr]
    -j, --poi-config=<json>   POI configuration
"#;

fn main() {
    mimir::logger_init().unwrap();
    let args: Args =
        docopt::Docopt::new(USAGE).and_then(|d| d.decode()).unwrap_or_else(|e| e.exit());

    let levels = args.flag_level
        .iter()
        .cloned()
        .collect();
    let city_level = args.flag_city_level;
    let mut parsed_pbf = parse_osm_pbf(&args.flag_input);
    debug!("creation of indexes");
    let mut rubber = Rubber::new(&args.flag_connection_string);

    info!("creating adminstrative regions");
    let admins_geofinder =
        administrative_regions(&mut parsed_pbf, levels).into_iter().collect::<AdminGeoFinder>();
    {
        info!("Extracting streets from osm");
        let mut streets = streets(&mut parsed_pbf, &admins_geofinder, city_level);

        info!("computing city weight");
        compute_admin_weight(&mut streets, &admins_geofinder);

        info!("computing street weight");
        compute_street_weight(&mut streets, city_level);

        if args.flag_import_way {
            info!("importing streets into Mimir");
            let nb_streets = rubber.index(&args.flag_dataset, streets.into_iter()).unwrap();
            info!("Nb of indexed street: {}", nb_streets);
        }
    }
    let nb_admins = rubber.index(&args.flag_dataset, admins_geofinder.admins()).unwrap();
    info!("Nb of indexed admin: {}", nb_admins);

    if args.flag_import_poi {
        let matcher = match args.flag_poi_config {
            None => PoiConfig::default(),
            Some(filename) => {
                let path = std::path::Path::new(&filename);
                let r = std::fs::File::open(&path).unwrap();
                PoiConfig::from_reader(r).unwrap()
            }
        };
        info!("Extracting pois from osm");
        let mut pois = pois(&mut parsed_pbf, &matcher, &admins_geofinder, city_level);

        info!("computing poi weight");
        compute_poi_weight(&mut pois, city_level);

        info!("Importing pois into Mimir");
        let nb_pois = rubber.index(&args.flag_dataset, pois.iter()).unwrap();

        info!("Nb of indexed pois: {}", nb_pois);
    }
}
