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
use super::osm_utils::get_way_coord;
use super::OsmPbfReader;
use crate::admin_geofinder::AdminGeoFinder;
use crate::{labels, utils, Error};
use bincode;
use failure::ResultExt;
use osmpbfreader::{OsmId, OsmObj, StoreObjs};
use rusqlite::{Connection, DropBehavior, ToSql, NO_PARAMS};
use slog_scope::{error, info};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

macro_rules! err_logger {
    ($obj:expr, $err_msg:expr) => {
        match $obj {
            Ok(x) => Some(x),
            Err(e) => {
                error!("{}: {}", $err_msg, e);
                None
            }
        }?
    };
    ($obj:expr, $err_msg:expr, $ret:expr) => {
        match $obj {
            Ok(x) => x,
            Err(e) => {
                error!("{}: {}", $err_msg, e);
                return $ret;
            }
        }
    };
}

macro_rules! get_kind {
    ($obj:expr) => {
        if $obj.is_node() {
            &0
        } else if $obj.is_way() {
            &1
        } else if $obj.is_relation() {
            &2
        } else {
            panic!("Unknown OSM object kind!")
        }
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
enum Kind {
    Node = 0,
    Way = 1,
    Relation = 2,
}

pub trait Getter {
    fn get(&self, key: &OsmId) -> Option<Cow<OsmObj>>;
}

impl Getter for BTreeMap<OsmId, OsmObj> {
    fn get(&self, key: &OsmId) -> Option<Cow<OsmObj>> {
        self.get(key).map(|x| Cow::Borrowed(x))
    }
}

struct DB<'a> {
    conn: Connection,
    db_file: &'a PathBuf,
    buffer: HashMap<OsmId, OsmObj>,
    db_buffer_size: usize,
}

impl<'a> DB<'a> {
    fn new(db_file: &'a PathBuf, db_buffer_size: usize) -> Result<DB<'a>, String> {
        let _ = fs::remove_file(db_file); // we ignore any potential error
        let conn = Connection::open(&db_file)
            .map_err(|e| format!("failed to open SQLITE connection: {}", e))?;

        conn.execute(
            "CREATE TABLE ids (
                id   INTEGER NOT NULL,
                obj  BLOB NOT NULL,
                kind INTEGER NOT NULL,
                UNIQUE(id, kind)
             )",
            NO_PARAMS,
        )
        .map_err(|e| format!("failed to create table: {}", e))?;
        Ok(DB {
            conn,
            db_file,
            buffer: HashMap::with_capacity(db_buffer_size),
            db_buffer_size,
        })
    }

    fn get_from_id(&self, id: &OsmId) -> Option<Cow<OsmObj>> {
        if let Some(obj) = self.buffer.get(id) {
            return Some(Cow::Borrowed(obj));
        }
        let mut stmt = err_logger!(
            self.conn
                .prepare("SELECT obj FROM ids WHERE id=?1 AND kind=?2"),
            "DB::get_from_id: prepare failed"
        );
        let mut iter = err_logger!(
            stmt.query(&[&id.inner_id() as &dyn ToSql, get_kind!(id)]),
            "DB::get_from_id: query_map failed"
        );
        while let Some(row) = err_logger!(iter.next(), "DB::get_from_id: next failed") {
            let obj: Vec<u8> = err_logger!(row.get(0), "DB::get_from_id: failed to get obj field");
            let obj: OsmObj = err_logger!(
                bincode::deserialize(&obj),
                "DB::for_each: serde conversion failed",
                None
            );
            return Some(Cow::Owned(obj));
        }
        None
    }

    #[allow(dead_code)]
    fn for_each<F: FnMut(Cow<OsmObj>)>(&self, mut f: F) {
        let mut values = self.buffer.values();
        while let Some(obj) = values.next() {
            f(Cow::Borrowed(obj));
        }
        let mut stmt = err_logger!(
            self.conn.prepare("SELECT obj FROM ids"),
            "DB::for_each: prepare failed",
            ()
        );
        let mut rows = err_logger!(stmt.query(NO_PARAMS), "DB::for_each: query_map failed", ());
        while let Some(row) = err_logger!(rows.next(), "DB::for_each: next failed", ()) {
            let obj: Vec<u8> = err_logger!(row.get(0), "DB::for_each: failed to get obj field", ());

            let obj: OsmObj = err_logger!(
                bincode::deserialize(&obj),
                "DB::for_each: serde conversion failed",
                ()
            );
            f(Cow::Owned(obj));
        }
    }

    fn for_each_filter<F: FnMut(Cow<OsmObj>)>(&self, filter: Kind, mut f: F) {
        self.buffer
            .values()
            .filter(|e| *get_kind!(e) == filter as i32)
            .for_each(|value| f(Cow::Borrowed(value)));
        let mut stmt = err_logger!(
            self.conn.prepare("SELECT obj FROM ids WHERE kind=?1"),
            "DB::for_each: prepare failed",
            ()
        );
        let mut rows = err_logger!(
            stmt.query(&[&(filter as i32) as &dyn ToSql]),
            "DB::for_each: query_map failed",
            ()
        );
        while let Some(row) = err_logger!(rows.next(), "DB::for_each: next failed", ()) {
            let obj: Vec<u8> = err_logger!(row.get(0), "DB::for_each: failed to get obj field", ());

            let obj: OsmObj = err_logger!(
                bincode::deserialize(&obj),
                "DB::for_each: serde conversion failed",
                ()
            );
            f(Cow::Owned(obj));
        }
    }

    fn flush_buffer(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let mut tx = err_logger!(
            self.conn.transaction(),
            "DB::flush_buffer: transaction creation failed",
            ()
        );
        tx.set_drop_behavior(DropBehavior::Ignore);

        {
            let mut buf = Vec::with_capacity(10);
            let mut req10 = "INSERT OR IGNORE INTO ids(id, obj, kind) VALUES (?1,?2,?3)".to_owned();
            for i in 1..10 {
                req10.push_str(&format!(",(?{},?{},?{})", i * 3 + 1, i * 3 + 2, i * 3 + 3));
            }
            let mut stmt10 = err_logger!(
                tx.prepare(&req10),
                "DB::flush_buffer: prepare failed",
                ()
            );
            let mut stmt = err_logger!(
                tx.prepare("INSERT OR IGNORE INTO ids(id, obj, kind) VALUES (?1,?2,?3)"),
                "DB::flush_buffer: prepare failed",
                ()
            );
            for (id, obj) in self.buffer.drain() {
                let ser_obj = match bincode::serialize(&obj) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("DB::flush_buffer: failed to convert to bincode: {}", e);
                        continue;
                    }
                };
                buf.push((id.inner_id(), get_kind!(obj), ser_obj));
                if buf.len() > 9 {
                    {
                        let mut objs = Vec::with_capacity(30);
                        for (id, kind, ser_obj) in buf.iter() {
                            objs.push(id as &dyn ToSql);
                            objs.push(ser_obj as &dyn ToSql);
                            objs.push(kind as &dyn ToSql);
                        }
                        if let Err(e) = stmt10.execute(&objs) {
                            error!("DB::flush_buffer: insert failed: {}", e);
                        }
                    }
                    buf.clear();
                }
            }
            for (id, kind, ser_obj) in buf.drain(..) {
                if let Err(e) = stmt.execute(&[&id as &dyn ToSql, &ser_obj, &kind]) {
                    error!("DB::flush_buffer: insert failed: {}", e);
                }
            }
        }
        err_logger!(
            tx.commit(),
            "DB::flush_buffer: transaction commit failed",
            ()
        );
    }
}

impl<'a> StoreObjs for DB<'a> {
    fn insert(&mut self, id: OsmId, obj: OsmObj) {
        if self.buffer.len() >= self.db_buffer_size {
            self.flush_buffer();
        }
        self.buffer.insert(id, obj);
    }

    fn contains_key(&self, id: &OsmId) -> bool {
        if self.buffer.contains_key(id) {
            return true;
        }
        let mut stmt = err_logger!(
            self.conn
                .prepare("SELECT id FROM ids WHERE id=?1 AND kind=?2"),
            "DB::contains_key: prepare failed",
            false
        );
        let mut iter = err_logger!(
            stmt.query(&[&id.inner_id() as &dyn ToSql, get_kind!(id)]),
            "DB::contains_key: query_map failed",
            false
        );
        err_logger!(iter.next(), "DB::contains_key: no row", false).is_some()
    }
}

impl<'a> Getter for DB<'a> {
    fn get(&self, key: &OsmId) -> Option<Cow<OsmObj>> {
        self.get_from_id(key)
    }
}

impl<'a> Drop for DB<'a> {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.db_file); // we ignore any potential error
    }
}

pub type AdminSet = BTreeSet<Arc<mimir::Admin>>;
pub type NameAdminMap = BTreeMap<StreetKey, Vec<osmpbfreader::OsmId>>;
pub type StreetsVec = Vec<mimir::Street>;
pub type StreetWithRelationSet = BTreeSet<osmpbfreader::OsmId>;

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct StreetKey {
    pub name: String,
    pub admins: AdminSet,
}

enum ObjWrapper<'a> {
    Map(BTreeMap<osmpbfreader::OsmId, osmpbfreader::OsmObj>),
    DB(DB<'a>),
}

impl<'a> ObjWrapper<'a> {
    fn new(db_file: &'a Option<PathBuf>, db_buffer_size: usize) -> Result<ObjWrapper<'a>, Error> {
        Ok(if let Some(ref db_file) = db_file {
            info!("Running with DB storage");
            ObjWrapper::DB(DB::new(db_file, db_buffer_size).map_err(|e| failure::err_msg(e))?)
        } else {
            info!("Running with BTreeMap (RAM) storage");
            ObjWrapper::Map(BTreeMap::new())
        })
    }

    #[allow(dead_code)]
    fn for_each<F: FnMut(Cow<OsmObj>)>(&self, mut f: F) {
        match *self {
            ObjWrapper::Map(ref m) => {
                let mut values = m.values();
                while let Some(value) = values.next() {
                    f(Cow::Borrowed(value));
                }
            }
            ObjWrapper::DB(ref db) => db.for_each(f),
        }
    }

    fn for_each_filter<F: FnMut(Cow<OsmObj>)>(&self, filter: Kind, mut f: F) {
        match *self {
            ObjWrapper::Map(ref m) => {
                m.values()
                    .filter(|e| *get_kind!(e) == filter as i32)
                    .for_each(|value| f(Cow::Borrowed(value)));
            }
            ObjWrapper::DB(ref db) => db.for_each_filter(filter, f),
        }
    }
}

impl<'a> Getter for ObjWrapper<'a> {
    fn get(&self, key: &OsmId) -> Option<Cow<OsmObj>> {
        match *self {
            ObjWrapper::Map(ref m) => m.get(key).map(|x| Cow::Borrowed(x)),
            ObjWrapper::DB(ref db) => db.get(key),
        }
    }
}

impl<'a> StoreObjs for ObjWrapper<'a> {
    fn insert(&mut self, id: OsmId, obj: OsmObj) {
        match *self {
            ObjWrapper::Map(ref mut m) => {
                m.insert(id, obj);
            }
            ObjWrapper::DB(ref mut db) => db.insert(id, obj),
        }
    }

    fn contains_key(&self, id: &OsmId) -> bool {
        match *self {
            ObjWrapper::Map(ref m) => m.contains_key(id),
            ObjWrapper::DB(ref db) => db.contains_key(id),
        }
    }
}

pub fn streets(
    pbf: &mut OsmPbfReader,
    admins_geofinder: &AdminGeoFinder,
    db_file: &Option<PathBuf>,
    db_buffer_size: usize,
) -> Result<StreetsVec, Error> {
    // This is the list of highway that we don't want to index
    // See [OSM Key Highway](https://wiki.openstreetmap.org/wiki/Key:highway) for background.
    const INVALID_HIGHWAY: &'static [&'static str] =
        &["bus_guideway", "escape", "bus_stop", "elevator", "platform"];

    // For the object to be a valid street, it needs to be an osm highway of a valid type,
    // or a relation of type associatedStreet.
    fn is_valid_obj(obj: &osmpbfreader::OsmObj) -> bool {
        match *obj {
            osmpbfreader::OsmObj::Way(ref way) => {
                way.tags.get("highway").map_or(false, |v| {
                    !v.is_empty() && !INVALID_HIGHWAY.iter().any(|&k| k == v)
                }) && way.tags.get("name").map_or(false, |v| !v.is_empty())
            }
            osmpbfreader::OsmObj::Relation(ref rel) => rel
                .tags
                .get("type")
                .map_or(false, |v| v == "associatedStreet"),
            _ => false,
        }
    }
    info!("reading pbf...");
    let mut objs_map = ObjWrapper::new(db_file, db_buffer_size)?;
    pbf.get_objs_and_deps_store(is_valid_obj, &mut objs_map)
        .context("Error occurred when reading pbf")?;
    info!("reading pbf done.");
    let mut street_rel: StreetWithRelationSet = BTreeSet::new();
    let mut street_list: StreetsVec = vec![];
    // Sometimes, streets can be divided into several "way"s that still have the same street name.
    // The reason why a street is divided may be that a part of the street become
    // a bridge/tunnel/etc. In this case, a "relation" tagged with (type = associatedStreet) is used
    // to group all these "way"s. In order not to have duplicates in autocompletion, we should tag
    // the osm ways in the relation not to index them twice.

    objs_map.for_each_filter(Kind::Relation, |obj| {
        let rel = obj.relation().expect("impossible unwrap failure occured");
        let way_name = rel.tags.get("name");
        rel.refs
            .iter()
            .filter(|ref_obj| ref_obj.member.is_way() && ref_obj.role == "street")
            .filter_map(|ref_obj| {
                let obj = objs_map.get(&ref_obj.member)?;
                let way = obj.way()?;
                let way_name = way_name.or_else(|| way.tags.get("name"))?;
                let admins = get_street_admin(admins_geofinder, &objs_map, way);
                let country_codes = utils::find_country_codes(admins.iter().map(|a| a.deref()));
                let street_label = labels::format_street_label(
                    &way_name,
                    admins.iter().map(|a| a.deref()),
                    &country_codes,
                );
                let coord = get_way_coord(&objs_map, way);
                Some(mimir::Street {
                    id: format!("street:osm:relation:{}", rel.id.0.to_string()),
                    name: way_name.to_string(),
                    label: street_label,
                    weight: 0.,
                    zip_codes: utils::get_zip_codes_from_admins(&admins),
                    administrative_regions: admins,
                    coord: get_way_coord(&objs_map, way),
                    approx_coord: Some(coord.into()),
                    distance: None,
                    country_codes,
                    context: None,
                })
            })
            .next()
            .map(|street| street_list.push(street));

        // Add osmid of all the relation members in the set
        // We don't create any street for all the osmid present in street_rel
        for ref_obj in &rel.refs {
            if ref_obj.member.is_way() {
                street_rel.insert(ref_obj.member);
            }
        }
    });

    // we merge all the ways with a key = way_name + admin list of level(=city_level)
    // we use a map NameAdminMap <key, value> to manage the merging of ways
    let mut name_admin_map = NameAdminMap::default();
    objs_map.for_each_filter(Kind::Way, |obj| {
        let osmid = obj.id();
        if street_rel.contains(&osmid) {
            return;
        }
        if let Some(way) = obj.way() {
            if let Some(name) = way.tags.get("name") {
                let name = name.to_string();
                let admins = get_street_admin(admins_geofinder, &objs_map, way)
                    .into_iter()
                    .filter(|admin| admin.is_city())
                    .collect();
                name_admin_map
                    .entry(StreetKey { name, admins })
                    .or_insert(vec![])
                    .push(osmid);
            }
        }
    });

    // Create a street for each way with osmid present in objs_map
    let streets = name_admin_map.values().filter_map(|way_ids| {
        let min_id = way_ids.iter().min()?;
        let obj = objs_map.get(&min_id)?;
        let way = obj.way()?;
        let name = way.tags.get("name")?.to_string();
        let admins = get_street_admin(admins_geofinder, &objs_map, way);

        let country_codes = utils::find_country_codes(admins.iter().map(|a| a.deref()));
        let street_label =
            labels::format_street_label(&name, admins.iter().map(|a| a.deref()), &country_codes);
        let coord = get_way_coord(&objs_map, way);
        Some(mimir::Street {
            id: format!("street:osm:way:{}", way.id.0.to_string()),
            label: street_label,
            name,
            weight: 0.,
            zip_codes: utils::get_zip_codes_from_admins(&admins),
            administrative_regions: admins,
            coord: get_way_coord(&objs_map, way),
            approx_coord: Some(coord.into()),
            distance: None,
            country_codes,
            context: None,
        })
    });
    street_list.extend(streets);

    Ok(street_list)
}

fn get_street_admin<T: StoreObjs + Getter>(
    admins_geofinder: &AdminGeoFinder,
    obj_map: &T,
    way: &osmpbfreader::objects::Way,
) -> Vec<Arc<mimir::Admin>> {
    /*
        To avoid corner cases where the ends of the way are near
        administrative boundaries, the geofinder is called
        on a middle node.
    */
    let nb_nodes = way.nodes.len();
    way.nodes
        .iter()
        .skip(nb_nodes / 2)
        .filter_map(|node_id| obj_map.get(&(*node_id).into()))
        .filter_map(|node_obj| {
            node_obj.node().map(|node| geo_types::Coordinate {
                x: node.lon(),
                y: node.lat(),
            })
        })
        .next()
        .map_or(vec![], |c| admins_geofinder.get(&c))
}

pub fn compute_street_weight(streets: &mut StreetsVec) {
    for st in streets {
        for admin in &mut st.administrative_regions {
            if admin.is_city() {
                st.weight = admin.weight;
                break;
            }
        }
    }
}
