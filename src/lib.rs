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

pub mod rubber;

#[macro_use]
extern crate log;

extern crate rustc_serialize;
use rustc_serialize::Encodable;

trait Incr: Clone {
    fn id(&self) -> &str;
    fn incr(&mut self);
}

#[derive(RustcDecodable, RustcEncodable, Debug, Clone)]
pub struct Coord {
    pub lat: f64,
    pub lon: f64,
}

#[derive(RustcDecodable, RustcEncodable, Debug, Clone)]
pub struct Admin {
    pub id: String,
    pub level: u32,
    pub name: String,
    pub zip_code: String,
    pub weight: u32,
    pub coord: Coord,
}
impl Incr for Admin {
    fn id(&self) -> &str {
        &self.id
    }
    fn incr(&mut self) {
        self.weight += 1;
    }
}

#[derive(RustcDecodable, RustcEncodable, Debug, Clone)]
pub struct Street {
    pub id: String,
    pub street_name: String,
    pub name: String,
    pub administrative_region: Admin,
    pub weight: u32,
}
impl Incr for Street {
    fn id(&self) -> &str {
        &self.id
    }
    fn incr(&mut self) {
        self.weight += 1;
    }
}

#[derive(RustcDecodable, RustcEncodable, Debug)]
pub struct Addr {
    pub id: String,
    pub house_number: String,
    pub street: Street,
    pub name: String,
    pub coord: Coord,
    pub weight: u32,
}
