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

extern crate curl;
extern crate rustc_serialize;

use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use self::rustc_serialize::json;
use rustc_serialize::Encodable;

use super::{Addr, Incr};


pub struct Rubber {
    pub index_name: String
}

impl Rubber {

    pub fn create_index(&self) -> Result<(), curl::ErrCode> {
        // first, we must delete with its own handle the old munin
        try!(curl::http::handle().delete("http://localhost:9200/".to_string() + &self.index_name).exec());

        let analysis = include_str!("../json/settings.json");
        assert!(analysis.parse::<json::Json>().is_ok());
        let res = try!(curl::http::handle().put("http://localhost:9200/munin", analysis).exec());
        assert!(res.get_code() == 200, "Error adding analysis: {}", res);

        Ok(())
    }

    fn push_bulk<'a, T: Encodable>(&self, s: &mut String, elt: &T) {
        s.push_str("{index: {}}\n");
        s.push_str(&json::encode(elt).unwrap());
        s.push('\n');
    }
    fn bulk_index<'a, T, I>(&self, url: &str, mut iter: I) -> Result<u32, curl::ErrCode>
        where T: Encodable, I: Iterator<Item = T>
    {
        let url = format!("{}/_bulk", url);
        let mut handle = curl::http::handle();
        let mut nb = 0;
        let mut chunk = String::new();
        loop {
            chunk.clear();
            let addr = match iter.next() { Some(a) => a, None => break };
            self.push_bulk(&mut chunk, &addr);
            nb += 1;
            for addr in iter.by_ref().take(1000) {
                self.push_bulk(&mut chunk, &addr);
                nb += 1;
            }
            let res = try!(handle.post(&*url, &chunk).exec());
            assert!(res.get_code() != 201, format!("result of bulk insert is not 201: {}", res));
        }
        Ok(nb)
    }

    fn upsert<T: Incr>(&self, elt: &T, map: &mut HashMap<String, T>) {
        match map.entry(elt.id().to_string()) {
            Vacant(e) => { e.insert(elt.clone()); }
            Occupied(mut e) => e.get_mut().incr()
        }
    }

    pub fn index<I: Iterator<Item = Addr>>(&self, iter: I) -> Result<u32, curl::ErrCode> {
        let mut admins = HashMap::new();
        let mut streets = HashMap::new();
        try!(self.bulk_index("http://localhost:9200/munin/addr", iter.inspect(|addr| {
            self.upsert(&addr.street.administrative_region, &mut admins);
            self.upsert(&addr.street, &mut streets);
        })));
        try!(self.bulk_index("http://localhost:9200/munin/admin", admins.into_iter().map(|e| e.1)));
        self.bulk_index("http://localhost:9200/munin/street", streets.into_iter().map(|e| e.1))
    }
}
