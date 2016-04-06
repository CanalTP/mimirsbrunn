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
extern crate csv;
extern crate rustc_serialize;
extern crate curl;
extern crate mimirsbrunn;

use std::path::Path;
use mimirsbrunn::rubber::Rubber;

#[derive(RustcDecodable, RustcEncodable)]
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
    pub fn insee(&self) -> &str {
        assert!(self.id.len() >= 5);
        &self.id[..5]
    }
    pub fn fantoir(&self) -> &str {
        assert!(self.id.len() >= 10);
        &self.id[..10]
    }
    pub fn into_addr(self) -> mimirsbrunn::Addr {
        let street_name = format!("{}, {} {}", self.street, self.zip, self.city);
        let addr_name = format!("{} {}", self.nb, street_name);
        let street_id = format!("street:{}", self.fantoir().to_string());
        let admin = mimirsbrunn::Admin {
            id: format!("admin:{}", self.insee()),
            level: 8,
            name: self.city,
            zip_code: self.zip,
            weight: 1,
            coord: mimirsbrunn::Coord {
                lat: 0.0,
                lon: 0.0,
            },
        };
        let street = mimirsbrunn::Street {
            id: street_id,
            street_name: self.street,
            name: street_name,
            administrative_region: admin,
            weight: 1,
        };
        mimirsbrunn::Addr {
            id: format!("addr:{};{}", self.lat, self.lon),
            house_number: self.nb,
            street: street,
            name: addr_name,
            coord: mimirsbrunn::Coord {
                lat: self.lat,
                lon: self.lon,
            },
            weight: 1,
        }
    }
}

fn index_bano(files: &[String]) {
    let rubber = Rubber {index_name: "munin".to_string()};

    println!("purge and create Munin...");
    rubber.create_index().unwrap();
    println!("Munin purged and created.");

    for f in files.iter() {
        println!("importing {}...", f);
        let mut rdr = csv::Reader::from_file(&Path::new(&f)).unwrap().has_headers(false);

        let iter = rdr.decode().map(|r| {
            let b: Bano = r.unwrap();
            b.into_addr()
        });
        let nb = rubber.index(iter).unwrap();
        println!("importing {}: {} addresses added.", f, nb);
    }
}

#[derive(RustcDecodable, Debug)]
struct Args {
    arg_bano_files: Vec<String>,
}

static USAGE: &'static str = "
Usage:
    bano2mimir <bano-files>...
";

fn main() {
    println!("importing bano into Mimir");

    let args: Args = docopt::Docopt::new(USAGE)
                         .and_then(|d| d.decode())
                         .unwrap_or_else(|e| e.exit());

    index_bano(&args.arg_bano_files);
}
