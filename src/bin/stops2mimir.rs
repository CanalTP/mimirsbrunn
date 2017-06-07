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

extern crate rustc_serialize;
extern crate docopt;
extern crate csv;
extern crate mimir;
extern crate itertools;
extern crate mimirsbrunn;
#[macro_use]
extern crate log;

use std::rc::Rc;
use mimir::rubber::Rubber;
use docopt::Docopt;
use mimirsbrunn::utils::{format_label, get_zip_codes_from_admins};
use mimirsbrunn::admin_geofinder::AdminGeoFinder;
use std::collections::HashMap;

const USAGE: &'static str = "
Usage:
    stops2mimir --help
    stops2mimir --input=<file> \
     [--connection-string=<connection-string>] [--dataset=<dataset>] [--city-level=<level>]

Options:
    -h, --help                Show this message.
    -i, --input=<file>        NTFS stops.txt file.
    -c, --connection-string=<connection-string>   \
                              Elasticsearch parameters [default: http://localhost:9200/munin].
    -d, --dataset=<dataset>   Name of the dataset [default: fr].
    -C, --city-level=<level>  City level to calculate weight [default: 8]
";

#[derive(Debug, RustcDecodable)]
struct Args {
    flag_input: String,
    flag_dataset: String,
    flag_connection_string: String,
    flag_city_level: u32,
}

struct StopPointIter<'a, R: std::io::Read + 'a> {
    iter: csv::StringRecords<'a, R>,
    stop_id_pos: usize,
    stop_lat_pos: usize,
    stop_lon_pos: usize,
    stop_name_pos: usize,
    location_type_pos: Option<usize>,
    stop_visible_pos: Option<usize>,
    parent_station_pos: Option<usize>,
    nb_stop_points: &'a mut HashMap<String, u32>,
}

impl<'a, R: std::io::Read + 'a> StopPointIter<'a, R> {
    fn new(r: &'a mut csv::Reader<R>,
           nb_stop_points: &'a mut HashMap<String, u32>)
           -> csv::Result<Self> {
        let headers = try!(r.headers());
        let get_optional_pos = |name| headers.iter().position(|s| s == name);

        let get_pos = |field| {
            get_optional_pos(field).ok_or_else(|| {
                csv::Error::Decode(format!("Invalid file, cannot find column '{}'", field))
            })
        };

        Ok(StopPointIter {
            iter: r.records(),
            stop_id_pos: try!(get_pos("stop_id")),
            stop_lat_pos: try!(get_pos("stop_lat")),
            stop_lon_pos: try!(get_pos("stop_lon")),
            stop_name_pos: try!(get_pos("stop_name")),
            location_type_pos: get_optional_pos("location_type"),
            stop_visible_pos: get_optional_pos("visible"),
            parent_station_pos: get_optional_pos("parent_station"),
            nb_stop_points: nb_stop_points,
        })
    }
    fn get_location_type(&self, record: &[String]) -> Option<u8> {
        self.location_type_pos.and_then(|pos| record.get(pos).and_then(|s| s.parse().ok()))
    }
    fn get_visible(&self, record: &[String]) -> Option<u8> {
        self.stop_visible_pos.and_then(|pos| record.get(pos).and_then(|s| s.parse().ok()))
    }

    fn get_parent_station<'b>(&self, record: &'b [String]) -> Option<&'b String> {
        self.parent_station_pos.and_then(|pos| record.get(pos))
    }
}

impl<'a, R: std::io::Read + 'a> Iterator for StopPointIter<'a, R> {
    type Item = csv::Result<mimir::Stop>;
    fn next(&mut self) -> Option<Self::Item> {
        fn get(record: &[String], pos: usize) -> csv::Result<&str> {
            record.get(pos)
                .map(|s| s.as_str())
                .ok_or_else(|| csv::Error::Decode(format!("Failed accessing record '{}'.", pos)))
        }
        fn parse_f64(s: &str) -> csv::Result<f64> {
            s.parse()
                .map_err(|_| csv::Error::Decode(format!("Failed converting '{}' from str.", s)))
        }

        fn is_valid_stop_area(location_type: &Option<u8>, visible: &Option<u8>) -> csv::Result<()> {
            if *location_type != Some(1) {
                Err(csv::Error::Decode("not a stop_area.".to_string()))
            } else if *visible == Some(0) {
                Err(csv::Error::Decode("stop_area invisible.".to_string()))
            } else {
                Ok(())
            }
        }

        self.iter.next().map(|r| {
            r.and_then(|r| {
                let location_type = self.get_location_type(&r);
                let visible = self.get_visible(&r);
                let parent_station = self.get_parent_station(&r);
                //if it's a stop point, we update its stop_area counter
                if let (Some(0), Some(id)) = (location_type, parent_station) {
                    if !id.is_empty() {
                        *self.nb_stop_points.entry(format!("stop_area:{}", id)).or_insert(0) += 1;
                    }
                }
                try!(is_valid_stop_area(&location_type, &visible));
                let stop_id = try!(get(&r, self.stop_id_pos));
                let stop_lat = try!(get(&r, self.stop_lat_pos));
                let stop_lat = try!(parse_f64(stop_lat));
                let stop_lon = try!(get(&r, self.stop_lon_pos));
                let stop_lon = try!(parse_f64(stop_lon));
                let stop_name = try!(get(&r, self.stop_name_pos));
                Ok(mimir::Stop {
                    id: format!("stop_area:{}", stop_id), // prefix to match navitia's id
                    coord: mimir::Coord::new(stop_lat, stop_lon),
                    label: stop_name.to_string(),
                    weight: 0.,
                    zip_codes: vec![],
                    administrative_regions: vec![],
                    name: stop_name.to_string(),
                })
            })
        })
    }
}

fn attach_stop(stop: &mut mimir::Stop, admins: Vec<Rc<mimir::Admin>>, city_level: u32) {
    stop.administrative_regions = admins;
    stop.label = format_label(&stop.administrative_regions, city_level, &stop.name);
    stop.zip_codes = get_zip_codes_from_admins(&stop.administrative_regions);
}

/// Attach the stops to administrative regions
///
/// The admins are loaded from Elasticsearch and stored in a quadtree
/// We attach a stop with all the admins that have a boundary containing
/// the coordinate of the stop
fn attach_stops_to_admins<'a, It: Iterator<Item = &'a mut mimir::Stop>>(stops: It,
                                                                        rubber: &mut Rubber,
                                                                        city_level: u32) {
    let admins = rubber.get_all_admins().unwrap_or_else(|_| {
        info!("Administratives regions not found in elasticsearch db");
        vec![]
    });

    info!("{} administrative regions loaded from mimir", admins.len());

    let admins_geofinder = admins.into_iter().collect::<AdminGeoFinder>();

    let mut nb_unmatched = 0u32;
    let mut nb_matched = 0u32;
    for mut stop in stops {
        let admins = admins_geofinder.get(&stop.coord);

        if admins.is_empty() {
            nb_unmatched += 1;
        } else {
            nb_matched += 1;
        }

        attach_stop(&mut stop, admins, city_level);
    }

    info!("there is {}/{} stops without any admin",
          nb_unmatched,
          nb_matched + nb_unmatched);
}

// Update weight value for each stop_area from HashMap.
fn finalize_stop_area_weight<'a, It: Iterator<Item = &'a mut mimir::Stop>>(
    stops: It,
    nb_stop_points: &HashMap<String, u32>)
{
    let max = *nb_stop_points.values().max().unwrap_or(&1) as f64;
    for stop in stops {
        if let Some(weight) = nb_stop_points.get(&stop.id) {
            stop.weight = *weight as f64 / max;
        }
    }
}

fn main() {
    mimir::logger_init().unwrap();
    info!("Launching stops2mimir...");

    let args: Args = Docopt::new(USAGE).and_then(|dopt| dopt.decode()).unwrap_or_else(|e| e.exit());

    info!("creation of indexes");
    let mut rubber = Rubber::new(&args.flag_connection_string);
    let mut rdr = csv::Reader::from_file(args.flag_input).unwrap().double_quote(true);

    let mut nb_stop_points = HashMap::new();

    let mut stops: Vec<mimir::Stop> = StopPointIter::new(&mut rdr, &mut nb_stop_points)
        .unwrap()
        .filter_map(|rc| {
            rc.map_err(|e| debug!("skip csv line because: {}", e))
                .ok()
        })
        .collect();

    attach_stops_to_admins(stops.iter_mut(), &mut rubber, args.flag_city_level);

    finalize_stop_area_weight(stops.iter_mut(), &nb_stop_points);

    info!("Importing stops into Mimir");
    let nb_stops = rubber.index(&args.flag_dataset, stops.iter()).unwrap();

    info!("Nb of indexed stops: {}", nb_stops);

}

#[test]
fn test_load_stops() {
    use itertools::Itertools;
    // stops.txt:
    // SP:main_station : StopPoint object
    // SA:main_station: StopArea valid
    // SA:second_station: StopArea valid with visible is empty
    // SA:invisible_station: invisible StopArea
    // SA:without_lat: StopArea object without lattitude coord
    // SA:witout_lon: StopArea object without longitude coord
    // SA:station_no_city: StopArea far away, we won't be able to attach it to a city
    let mut rdr = csv::Reader::from_file("./tests/fixtures/stops.txt".to_string())
        .unwrap()
        .double_quote(true);

    let mut nb_stop_points = HashMap::new();
    let stops: Vec<mimir::Stop> = StopPointIter::new(&mut rdr, &mut nb_stop_points)
        .unwrap()
        .filter_map(|rc| {
            rc.map_err(|e| println!("error at csv line decoding : {}", e))
                .ok()
        })
        .collect();
    assert_eq!(stops.len(), 5);
    let ids: Vec<_> = stops.iter().map(|s| s.id.clone()).sorted();
    assert_eq!(ids,
               vec!["stop_area:SA:main_station",
                    "stop_area:SA:second_station",
                    "stop_area:SA:station_no_city",
                    "stop_area:SA:weight_1_station",
                    "stop_area:SA:weight_3_station"]);
}
