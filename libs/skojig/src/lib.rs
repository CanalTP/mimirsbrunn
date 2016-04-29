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

#![cfg_attr(feature = "serde_macros", feature(custom_derive, plugin))]
#![cfg_attr(feature = "serde_macros", plugin(serde_macros))]

extern crate serde;
extern crate serde_json;

extern crate rustc_serialize;
extern crate curl;
extern crate docopt;
extern crate iron;
extern crate urlencoded;

extern crate rustless;
extern crate hyper;
extern crate jsonway;
extern crate valico;

use iron::Iron;
use rustless::Application;

#[macro_use]
extern crate mdo;
#[macro_use]
extern crate log;


mod api;
mod query;
mod model;

pub fn runserver() {
    let api = api::root();
    let app = Application::new(api);
    Iron::new(app).http("0.0.0.0:4000").unwrap();
}
