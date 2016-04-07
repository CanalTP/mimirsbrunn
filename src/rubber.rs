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
extern crate rs_es;

use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use self::rustc_serialize::json;
use rustc_serialize::Encodable;

use super::{Addr, Incr};

pub struct Rubber {
    pub index_name: String,
    client: rs_es::Client
}

impl Rubber {
    pub fn new(host: String, port: u32, index: String) -> Rubber {
        Rubber {
            index_name: index,
            client: rs_es::Client::new(&host, port)
        }
    }

    pub fn create_index(&mut self) -> Result<(), curl::ErrCode> {
        debug!("creating index");
        self.client.delete_index(&self.index_name).unwrap();  // TODO handle error
        // first, we must delete with its own handle the old munin

        // Note: for the moment I don't see an easy way to do this with rs_es
        let analysis = include_str!("../json/settings.json");
        assert!(analysis.parse::<json::Json>().is_ok());
        let res = try!(curl::http::handle().put("http://localhost:9200/munin", analysis).exec());
        assert!(res.get_code() == 200, "Error adding analysis: {}", res);

        Ok(())
    }

    fn bulk_index<'a, T, I>(&mut self, type_name: &str, mut iter: I) -> Result<u32, curl::ErrCode>
        where T: Encodable, I: Iterator<Item = T>
    {
        use self::rs_es::operations::bulk::Action;
        let mut chunk = Vec::new();
        let mut nb = 0;

        loop {
            chunk.clear();
            let addr = match iter.next() { Some(a) => a, None => break };
            chunk.push(Action::index(json::encode(&addr).unwrap()));

            nb += 1;
            for addr in iter.by_ref().take(1000) {
                chunk.push(Action::index(json::encode(&addr).unwrap()));
                nb += 1;
            }
            self.client.bulk(&chunk).with_index(&self.index_name).with_doc_type(type_name).send().unwrap(); //TODO use result
        }

        Ok(nb)
    }

    pub fn index<I: Iterator<Item = Addr>>(&mut self, iter: I) -> Result<u32, curl::ErrCode> {
        let mut admins = HashMap::new();
        let mut streets = HashMap::new();


        try!(self.bulk_index("addr", iter.inspect(|addr| {
            upsert(&addr.street.administrative_region, &mut admins);
            upsert(&addr.street, &mut streets);
        })));
        try!(self.bulk_index("admin", admins.into_iter().map(|e| e.1)));
        self.bulk_index("street", streets.into_iter().map(|e| e.1))
    }
}


fn upsert<T: Incr>(elt: &T, map: &mut HashMap<String, T>) {
    match map.entry(elt.id().to_string()) {
        Vacant(e) => { e.insert(elt.clone()); }
        Occupied(mut e) => e.get_mut().incr()
    }
}
