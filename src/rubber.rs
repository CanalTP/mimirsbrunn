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
extern crate rs_es;
extern crate serde;
extern crate serde_json;
extern crate regex;

use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};

use super::objects::{Addr, Incr, DocType};

// Rubber is an wrapper around elasticsearch API
pub struct Rubber {
    index_name: String,
    client: rs_es::Client,
}

impl Rubber {
    // build a rubber with a connection string (http://host:port/index)
    pub fn new(cnx: &str) -> Rubber {
        let re = regex::Regex::new(r"(?P<host>.+?):(?P<port>\d{4})/(?P<index>\w+)").unwrap();
        let cap = re.captures(cnx).unwrap();
        let host = cap.name("host").unwrap();
        let port = cap.name("port").unwrap().parse::<u32>().unwrap();
        let index = cap.name("index").unwrap();
        info!("elastic search host {:?} port {:?} index {:?}",
              host,
              port,
              index);

        Rubber {
            index_name: index.to_string(),
            client: rs_es::Client::new(&host, port),
        }
    }

    pub fn create_index(&mut self) -> Result<(), rs_es::error::EsError> {
        debug!("creating index");
        match self.client.delete_index(&self.index_name) {
            Err(e) => info!("unable to remove index, {}", e),
            _ => (),
        }
        // first, we must delete with its own handle the old munin

        // Note: for the moment I don't see an easy way to do this with rs_es
        let analysis = include_str!("../json/settings.json");
        // assert!(analysis.parse::<json::Json>().is_ok());
        let res = curl::http::handle().put("http://localhost:9200/munin", analysis).exec().unwrap();

        assert!(res.get_code() == 200, "Error adding analysis: {}", res);

        Ok(())
    }

    pub fn bulk_index<T, I>(&mut self, mut iter: I) -> Result<u32, rs_es::error::EsError>
        where T: serde::Serialize + DocType,
              I: Iterator<Item = T>
    {
        use self::rs_es::operations::bulk::Action;
        let mut chunk = Vec::new();
        let mut nb = 0;

        loop {
            chunk.clear();
            let addr = match iter.next() {
                Some(a) => a,
                None => break,
            };
            chunk.push(Action::index(addr));

            nb += 1;
            for addr in iter.by_ref().take(1000) {
                chunk.push(Action::index(addr));
                nb += 1;
            }
            try!(self.client
                     .bulk(&chunk)
                     .with_index(&self.index_name)
                     .with_doc_type(T::doc_type())
                     .send());
        }

        Ok(nb)
    }

    pub fn index<I: Iterator<Item = Addr>>(&mut self,
                                           iter: I)
                                           -> Result<u32, rs_es::error::EsError> {
        let mut admins = HashMap::new();
        let mut streets = HashMap::new();
        let mut nb = 0;

        nb += try!(self.bulk_index(iter.inspect(|addr| {
            upsert(&addr.street.administrative_region, &mut admins);
            upsert(&addr.street, &mut streets);
        })));
        nb += try!(self.bulk_index(admins.values()));
        nb += try!(self.bulk_index(streets.values()));
        Ok(nb)
    }
}


fn upsert<T: Incr>(elt: &T, map: &mut HashMap<String, T>) {
    match map.entry(elt.id().to_string()) {
        Vacant(e) => {
            e.insert(elt.clone());
        }
        Occupied(mut e) => e.get_mut().incr(),
    }
}
