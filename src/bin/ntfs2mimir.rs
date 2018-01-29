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

#[macro_use]
extern crate log;
extern crate mimir;
extern crate mimirsbrunn;
extern crate navitia_model;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;

use std::path::PathBuf;
use structopt::StructOpt;
use mimirsbrunn::stops::*;
use navitia_model::objects as navitia;
use navitia_model::collection::Idx;

#[derive(Debug, StructOpt)]
struct Args {
    /// NTFS directory.
    #[structopt(short = "i", long = "input", parse(from_os_str), default_value = ".")]
    input: PathBuf,
    /// Name of the dataset.
    #[structopt(short = "d", long = "dataset", default_value = "fr")]
    dataset: String,
    /// Elasticsearch parameters.
    #[structopt(short = "c", long = "connection-string",
                default_value = "http://localhost:9200/munin")]
    connection_string: String,
    /// Deprecated option.
    #[structopt(short = "C", long = "city-level", default_value = "8")]
    city_level: Option<u32>,
}

fn to_mimir(
    idx: Idx<navitia::StopArea>,
    stop_area: &navitia::StopArea,
    navitia: &navitia_model::PtObjects,
) -> mimir::Stop {
    let commercial_modes = navitia
        .get_corresponding_from_idx(idx)
        .into_iter()
        .map(|cm_idx| mimir::CommercialMode {
            id: format!("commercial_mode:{}", navitia.commercial_modes[cm_idx].id),
            name: navitia.commercial_modes[cm_idx].name.clone(),
        })
        .collect();
    let physical_modes = navitia
        .get_corresponding_from_idx(idx)
        .into_iter()
        .map(|pm_idx| mimir::PhysicalMode {
            id: format!("physical_mode:{}", navitia.physical_modes[pm_idx].id),
            name: navitia.physical_modes[pm_idx].name.clone(),
        })
        .collect();
    mimir::Stop {
        id: format!("stop_area:{}", stop_area.id),
        label: stop_area.name.clone(),
        name: stop_area.name.clone(),
        coord: mimir::Coord::new(stop_area.coord.lat, stop_area.coord.lon),
        commercial_modes: commercial_modes,
        physical_modes: physical_modes,
        administrative_regions: vec![],
        weight: 0.,
        zip_codes: vec![],
        coverages: vec![],
    }
}

fn main() {
    mimir::logger_init();
    info!("Launching ntfs2mimir...");

    let args = Args::from_args();
    if args.city_level.is_some() {
        warn!("city-level option is deprecated, it now has no effect.");
    }
    
    let navitia = navitia_model::ntfs::read(&args.input);
    let nb_stop_points = navitia
        .stop_areas
        .iter()
        .map(|(idx, sa)| {
            let id = format!("stop_area:{}", sa.id);
            let nb_stop_points = navitia
                .get_corresponding_from_idx::<_, navitia::StopPoint>(idx)
                .len();
            (id, nb_stop_points as u32)
        })
        .collect();
    let mut stops: Vec<mimir::Stop> = navitia
        .stop_areas
        .iter()
        .map(|(idx, sa)| to_mimir(idx, sa, &navitia))
        .collect();
    set_weights(stops.iter_mut(), &nb_stop_points);
    import_stops(stops, &args.connection_string, &args.dataset);
}
