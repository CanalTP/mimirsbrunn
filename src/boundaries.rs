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
extern crate log;
extern crate osmpbfreader;
extern crate mimir;
extern crate osm_builder;

use std::collections::BTreeMap;
use geo::{Polygon, MultiPolygon, LineString, Coordinate, Point};
use geo::algorithm::centroid::Centroid;

#[cfg(test)]
use osm_builder::named_node;

struct BoundaryPart {
    nodes: Vec<osmpbfreader::Node>,
}

impl BoundaryPart {
    pub fn new(nodes: Vec<osmpbfreader::Node>) -> BoundaryPart {
        BoundaryPart { nodes: nodes }
    }
    pub fn first(&self) -> i64 {
        self.nodes.first().unwrap().id
    }
    pub fn last(&self) -> i64 {
        self.nodes.last().unwrap().id
    }
}

fn get_nodes(way: &osmpbfreader::Way,
             objects: &BTreeMap<osmpbfreader::OsmId, osmpbfreader::OsmObj>)
             -> Vec<osmpbfreader::Node> {
    way.nodes
        .iter()
        .filter_map(|node_id| objects.get(&osmpbfreader::OsmId::Node(*node_id)))
        .filter_map(|node_obj| {
            if let &osmpbfreader::OsmObj::Node(ref node) = node_obj {
                Some(node.clone())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn test_get_nodes() {
    let mut objects = BTreeMap::new();
    let way = osmpbfreader::Way {
        id: 12,
        nodes: vec![12, 15, 8, 68],
        tags: osmpbfreader::Tags::new(),
    };
    objects.insert(osmpbfreader::OsmId::Way(12),
                   osmpbfreader::OsmObj::Way(way.clone()));
    let node_12 = osmpbfreader::Node {
        id: 12,
        lat: 1.2,
        lon: 3.7,
        tags: osmpbfreader::Tags::new(),
    };
    objects.insert(osmpbfreader::OsmId::Node(12),
                   osmpbfreader::OsmObj::Node(node_12));
    let node_13 = osmpbfreader::Node {
        id: 13,
        lat: 1.5,
        lon: 3.5,
        tags: osmpbfreader::Tags::new(),
    };
    objects.insert(osmpbfreader::OsmId::Node(13),
                   osmpbfreader::OsmObj::Node(node_13));
    let node_15 = osmpbfreader::Node {
        id: 15,
        lat: 7.5,
        lon: 13.5,
        tags: osmpbfreader::Tags::new(),
    };
    objects.insert(osmpbfreader::OsmId::Node(15),
                   osmpbfreader::OsmObj::Node(node_15));
    let node_8 = osmpbfreader::Node {
        id: 8,
        lat: 5.5,
        lon: 63.5,
        tags: osmpbfreader::Tags::new(),
    };
    objects.insert(osmpbfreader::OsmId::Node(8),
                   osmpbfreader::OsmObj::Node(node_8));
    let node_68 = osmpbfreader::Node {
        id: 68,
        lat: 45.5,
        lon: 53.5,
        tags: osmpbfreader::Tags::new(),
    };
    objects.insert(osmpbfreader::OsmId::Node(68),
                   osmpbfreader::OsmObj::Node(node_68));

    let nodes = get_nodes(&way, &objects);
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[0].id, 12);
    assert_eq!(nodes[1].id, 15);
    assert_eq!(nodes[2].id, 8);
    assert_eq!(nodes[3].id, 68);
}

pub fn build_boundary(relation: &osmpbfreader::Relation,
                      objects: &BTreeMap<osmpbfreader::OsmId, osmpbfreader::OsmObj>)
                      -> Option<MultiPolygon> {
    let roles = vec!["outer".to_string(), "enclave".to_string()];
    let mut boundary_parts: Vec<BoundaryPart> = relation.refs
        .iter()
        .filter(|rf| roles.contains(&rf.role))
        .filter_map(|refe| {
            objects.get(&refe.member).or_else(|| {
                warn!("missing element for relation {}", relation.id);
                None
            })
        })
        .filter_map(|way_obj| {
            if let &osmpbfreader::OsmObj::Way(ref way) = way_obj {
                Some(way)
            } else {
                None
            }
        })
        .map(|way| get_nodes(&way, objects))
        .filter(|nodes| nodes.len() > 1)
        .map(|nodes| BoundaryPart::new(nodes))
        .collect();
    let mut multipoly = MultiPolygon(vec![]);
    // we want to try build a polygon for a least each way
    while !boundary_parts.is_empty() {
        let mut current = -1;
        let mut first = -2;
        let mut nb_try = 0;

        let mut outer: Vec<osmpbfreader::Node> = Vec::new();
        // we try to close the polygon, if we can't we want to at least have tried one time per
        // way. We could improve that latter by trying to attach the way to both side of the
        // polygon
        let max_try = boundary_parts.len();
        while current != first && nb_try < max_try {
            let mut i = 0;
            while i < boundary_parts.len() {
                if outer.is_empty() {
                    // our polygon is empty, we initialise it with the current way
                    first = boundary_parts[i].first();
                    current = boundary_parts[i].last();
                    outer.append(&mut boundary_parts[i].nodes);
                    // this way has been used, we remove it from the pool
                    boundary_parts.remove(i);
                    continue;
                }
                if current == boundary_parts[i].first() {
                    // the start of current way touch the polygon, we add it and remove it from the
                    // pool
                    current = boundary_parts[i].last();
                    outer.append(&mut boundary_parts[i].nodes);
                    boundary_parts.remove(i);
                } else if current == boundary_parts[i].last() {
                    // the end of the current way touch the polygon, we reverse the way and add it
                    current = boundary_parts[i].first();
                    boundary_parts[i].nodes.reverse();
                    outer.append(&mut boundary_parts[i].nodes);
                    boundary_parts.remove(i);
                } else {
                    i += 1;
                    // didnt do anything, we want to explore the next way, if we had do something we
                    // will have removed the current way and there will be no need to increment
                }
                if current == first {
                    // our polygon is closed, we create it and add it to the multipolygon
                    let polygon = Polygon(LineString(outer.iter()
                                              .map(|n| {
                                                  Point(Coordinate {
                                                      x: n.lat,
                                                      y: n.lon,
                                                  })
                                              })
                                              .collect()),
                                          vec![]);
                    multipoly.0.push(polygon);
                    break;
                }
            }
            nb_try += 1;
        }
    }
    if multipoly.0.is_empty() {
        None
    } else {
        Some(multipoly)
    }
}

pub fn make_centroid(boundary: &Option<MultiPolygon>) -> mimir::Coord {
    boundary.as_ref()
        .and_then(|b| b.centroid().map(|c| mimir::Coord(c.0)))
        .unwrap_or(mimir::Coord::new(0., 0.))
}

#[test]
fn test_build_bounadry_empty() {
    let objects = BTreeMap::new();
    let mut relation = osmpbfreader::Relation {
        id: 12,
        refs: vec![],
        tags: osmpbfreader::Tags::new(),
    };
    relation.refs.push(osmpbfreader::Ref {
        member: osmpbfreader::OsmId::Way(4),
        role: "outer".to_string(),
    });
    relation.refs.push(osmpbfreader::Ref {
        member: osmpbfreader::OsmId::Way(65),
        role: "outer".to_string(),
    });
    relation.refs.push(osmpbfreader::Ref {
        member: osmpbfreader::OsmId::Way(22),
        role: "".to_string(),
    });
    assert!(build_boundary(&relation, &objects).is_none());
}

#[test]
fn test_build_bounadry_not_closed() {
    let mut builder = osm_builder::OsmBuilder::new();
    let rel_id: osmpbfreader::OsmId;
    {
        let mut rel = builder.relation();
        rel_id = rel.relation_id;
        rel.outer(vec![named_node(3.4, 5.2, "start"), named_node(5.4, 5.1, "1")])
            .outer(vec![named_node(5.4, 5.1, "1"), named_node(2.4, 3.1, "2")])
            .outer(vec![named_node(2.4, 3.2, "2"), named_node(6.4, 6.1, "end")]);
    }
    if let osmpbfreader::OsmObj::Relation(ref relation) = builder.objects[&rel_id] {
        assert!(build_boundary(&relation, &builder.objects).is_none());
    } else {
        assert!(false);//this should not happen
    }
}

#[test]
fn test_build_bounadry_closed() {
    let mut builder = osm_builder::OsmBuilder::new();
    let rel_id: osmpbfreader::OsmId;
    {
        let mut rel = builder.relation();
        rel_id = rel.relation_id;
        rel.outer(vec![named_node(3.4, 5.2, "start"), named_node(5.4, 5.1, "1")])
            .outer(vec![named_node(5.4, 5.1, "1"), named_node(2.4, 3.1, "2")])
            .outer(vec![named_node(2.4, 3.2, "2"), named_node(6.4, 6.1, "start")]);
    }
    if let osmpbfreader::OsmObj::Relation(ref relation) = builder.objects[&rel_id] {
        let multipolygon = build_boundary(&relation, &builder.objects);
        assert!(multipolygon.is_some());
        let multipolygon = multipolygon.unwrap();
        assert_eq!(multipolygon.0.len(), 1);

    } else {
        assert!(false);//this should not happen
    }
}
