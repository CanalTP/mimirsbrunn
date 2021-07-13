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
use mimir::rubber::IndexSettings;
use mimirsbrunn::stops::*;
use slog_scope::{info, warn};
use std::cmp::Ordering;
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::BuildHasherDefault;
use std::path::PathBuf;
use structopt::StructOpt;
use transit_model::objects as navitia;
use typed_index_collection::Idx;

#[derive(Debug, StructOpt)]
struct Args {
    /// NTFS directory.
    #[structopt(short = "i", long = "input", parse(from_os_str), default_value = ".")]
    input: PathBuf,
    /// Name of the dataset.
    #[structopt(short = "d", long = "dataset", default_value = "fr")]
    dataset: String,
    /// Elasticsearch parameters.
    #[structopt(
        short = "c",
        long = "connection-string",
        default_value = "http://localhost:9200/munin"
    )]
    connection_string: String,
    /// Deprecated option.
    #[structopt(short = "C", long = "city-level")]
    city_level: Option<String>,
    /// Number of shards for the es index
    #[structopt(short = "s", long = "nb-shards", default_value = "1")]
    nb_shards: usize,
    /// Number of replicas for the es index
    #[structopt(short = "r", long = "nb-replicas", default_value = "1")]
    nb_replicas: usize,
}

fn get_lines(idx: Idx<navitia::StopArea>, navitia: &transit_model::Model) -> Vec<mimir::Line> {
    use mimir::FromTransitModel;
    let mut lines: Vec<_> = navitia
        .get_corresponding_from_idx(idx)
        .into_iter()
        .map(|l_idx| mimir::Line::from_transit_model(l_idx, navitia))
        .collect();

    // we want the lines to be sorted in a way where
    // line-3 is before line-11, so be use a human_sort
    lines.sort_by(|lhs, rhs| {
        match (&lhs.sort_order, &rhs.sort_order) {
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(s), Some(o)) => s.cmp(o),
            (None, None) => Ordering::Equal,
        }
        .then_with(|| match (&lhs.code, &rhs.code) {
            (Some(l), Some(r)) => human_sort::compare(l, r),
            _ => Ordering::Equal,
        })
        .then_with(|| human_sort::compare(&lhs.name, &rhs.name))
    });
    lines
}

fn to_mimir(
    idx: Idx<navitia::StopArea>,
    stop_area: &navitia::StopArea,
    navitia: &transit_model::Model,
) -> mimir::Stop {
    let commercial_modes = navitia
        .get_corresponding_from_idx(idx)
        .into_iter()
        .map(|cm_idx| mimir::CommercialMode {
            id: mimir::objects::normalize_id(
                "commercial_mode",
                &navitia.commercial_modes[cm_idx].id,
            ),
            name: navitia.commercial_modes[cm_idx].name.clone(),
        })
        .collect();
    let physical_modes = navitia
        .get_corresponding_from_idx(idx)
        .into_iter()
        .map(|pm_idx| mimir::PhysicalMode {
            id: mimir::objects::normalize_id("physical_mode", &navitia.physical_modes[pm_idx].id),
            name: navitia.physical_modes[pm_idx].name.clone(),
        })
        .collect();
    let comments = stop_area
        .comment_links
        .iter()
        .filter_map(|comment_id| {
            let res = navitia.comments.get(comment_id);
            if res.is_none() {
                warn!("Could not retrieve comments for id {}", comment_id);
            }
            res
        })
        .map(|comment| mimir::Comment {
            name: comment.name.clone(),
        })
        .collect();
    let feed_publishers = navitia
        .get_corresponding_from_idx(idx)
        .into_iter()
        .map(|contrib_idx| mimir::FeedPublisher {
            id: navitia.contributors[contrib_idx].id.clone(),
            name: navitia.contributors[contrib_idx].name.clone(),
            license: navitia.contributors[contrib_idx]
                .license
                .clone()
                .unwrap_or_else(|| "".into()),
            url: navitia.contributors[contrib_idx]
                .website
                .clone()
                .unwrap_or_else(|| "".into()),
        })
        .collect();
    let coord = mimir::Coord::new(stop_area.coord.lon, stop_area.coord.lat);

    let lines = get_lines(idx, navitia);

    mimir::Stop {
        id: mimir::objects::normalize_id("stop_area", &stop_area.id),
        label: stop_area.name.clone(),
        name: stop_area.name.clone(),
        coord,
        approx_coord: Some(coord.into()),
        commercial_modes,
        physical_modes,
        lines,
        comments,
        timezone: stop_area
            .timezone
            .clone()
            .map(chrono_tz::Tz::name)
            .map(str::to_owned)
            .unwrap_or_default(),
        codes: stop_area
            .codes
            .iter()
            .map(|&(ref t, ref v)| mimir::Code {
                name: t.clone(),
                value: v.clone(),
            })
            .collect(),
        properties: stop_area
            .object_properties
            .iter()
            .map(|(ref k, ref v)| mimir::Property {
                key: k.to_string(),
                value: v.to_string(),
            })
            .collect(),
        feed_publishers,
        ..Default::default()
    }
}

fn main() {
    mimirsbrunn::utils::launch_run(run);
}

fn run(args: Args) -> Result<(), transit_model::Error> {
    info!("Launching ntfs2mimir...");

    if args.city_level.is_some() {
        warn!("city-level option is deprecated, it now has no effect.");
    }

    let navitia = transit_model::ntfs::read(&args.input)?;

    let nb_stop_points: HashMap<String, u32, BuildHasherDefault<DefaultHasher>> = navitia
        .stop_areas
        .iter()
        .map(|(idx, sa)| {
            let id = mimir::objects::normalize_id("stop_area", &sa.id);
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
    initialize_weights(stops.iter_mut(), &nb_stop_points);

    let index_settings = IndexSettings {
        nb_shards: args.nb_shards,
        nb_replicas: args.nb_replicas,
    };

    import_stops(
        stops,
        &args.connection_string,
        &args.dataset,
        index_settings,
    )
    .with_context(|err| {
        format!(
            "Error occurred when importing stops into {} on {}: {}",
            args.dataset, args.connection_string, err
        )
    })?;
    Ok(())
}

#[test]
fn test_bad_connection_string() {
    let args = Args {
        input: PathBuf::from("./tests/fixtures/ntfs"),
        connection_string: "http://localhost:1".to_string(),
        dataset: "bob".to_string(),
        city_level: None,
        nb_replicas: 1,
        nb_shards: 1,
    };
    let causes = run(args)
        .unwrap_err()
        .iter_chain()
        .map(|cause| format!("{}", cause))
        .collect::<Vec<String>>();
    assert_eq!(
        causes,
        [
            "Error occurred when importing stops into bob on http://localhost:1: Error: HTTP Error while creating template template_addr".to_string(),
            "Error: HTTP Error while creating template template_addr".to_string(),
        ]
    );
}

#[test]
fn test_bad_file() {
    let args = Args {
        input: PathBuf::from("./tests/fixtures/not_exist"),
        connection_string: "http://localhost:9200".to_string(),
        dataset: "bob".to_string(),
        city_level: None,
        nb_replicas: 1,
        nb_shards: 1,
    };
    let causes = run(args)
        .unwrap_err()
        .iter_chain()
        .map(|cause| format!("{}", cause))
        .collect::<Vec<String>>();
    assert_eq!(
        causes,
        [
            "file \"./tests/fixtures/not_exist\" is neither a file nor a directory, cannot read a ntfs from it"
        ]
    );
}
