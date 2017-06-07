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
    pub fn first(&self) -> osmpbfreader::NodeId {
        self.nodes
            .first()
            .unwrap()
            .id
    }
    pub fn last(&self) -> osmpbfreader::NodeId {
        self.nodes
            .last()
            .unwrap()
            .id
    }
}

fn get_nodes(way: &osmpbfreader::Way,
             objects: &BTreeMap<osmpbfreader::OsmId, osmpbfreader::OsmObj>)
             -> Vec<osmpbfreader::Node> {
    way.nodes
        .iter()
        .filter_map(|node_id| objects.get(&osmpbfreader::OsmId::Node(*node_id)))
        .filter_map(|node_obj| if let osmpbfreader::OsmObj::Node(ref node) = *node_obj {
            Some(node.clone())
        } else {
            None
        })
        .collect()
}

#[test]
fn test_get_nodes() {
    let mut objects = BTreeMap::new();
    let way = osmpbfreader::Way {
        id: osmpbfreader::WayId(12),
        nodes: [12, 15, 8, 68].iter().map(|&id| osmpbfreader::NodeId(id)).collect(),
        tags: osmpbfreader::Tags::new(),
    };
    objects.insert(way.id.into(), way.clone().into());
    let node_12 = osmpbfreader::Node {
        id: osmpbfreader::NodeId(12),
        decimicro_lat: 12000000,
        decimicro_lon: 37000000,
        tags: osmpbfreader::Tags::new(),
    };
    objects.insert(node_12.id.into(), node_12.into());
    let node_13 = osmpbfreader::Node {
        id: osmpbfreader::NodeId(13),
        decimicro_lat: 15000000,
        decimicro_lon: 35000000,
        tags: osmpbfreader::Tags::new(),
    };
    objects.insert(node_13.id.into(), node_13.into());
    let node_15 = osmpbfreader::Node {
        id: osmpbfreader::NodeId(15),
        decimicro_lat: 75000000,
        decimicro_lon: 135000000,
        tags: osmpbfreader::Tags::new(),
    };
    objects.insert(node_15.id.into(), node_15.into());
    let node_8 = osmpbfreader::Node {
        id: osmpbfreader::NodeId(8),
        decimicro_lat: 55000000,
        decimicro_lon: 635000000,
        tags: osmpbfreader::Tags::new(),
    };
    objects.insert(node_8.id.into(), node_8.into());
    let node_68 = osmpbfreader::Node {
        id: osmpbfreader::NodeId(68),
        decimicro_lat: 455000000,
        decimicro_lon: 535000000,
        tags: osmpbfreader::Tags::new(),
    };
    objects.insert(node_68.id.into(), node_68.into());

    let nodes = get_nodes(&way, &objects);
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[0].id.0, 12);
    assert_eq!(nodes[1].id.0, 15);
    assert_eq!(nodes[2].id.0, 8);
    assert_eq!(nodes[3].id.0, 68);
}

pub fn build_boundary(relation: &osmpbfreader::Relation,
                      objects: &BTreeMap<osmpbfreader::OsmId, osmpbfreader::OsmObj>)
                      -> Option<MultiPolygon<f64>> {
    let roles = ["outer", "enclave", ""];
    let mut boundary_parts: Vec<BoundaryPart> = relation.refs
        .iter()
        .filter(|r| roles.contains(&r.role.as_str()))
        .filter_map(|r| {
            let obj = objects.get(&r.member);
            if obj.is_none() {
                debug!("missing element {:?} for relation {}",
                       r.member,
                       relation.id.0);
            }
            obj
        })
        .filter_map(|way_obj| way_obj.way())
        .map(|way| get_nodes(way, objects))
        .filter(|nodes| nodes.len() > 1)
        .map(BoundaryPart::new)
        .collect();
    let mut multipoly = MultiPolygon(vec![]);
    // we want to try build a polygon for a least each way
    while !boundary_parts.is_empty() {
        let first_part = boundary_parts.remove(0);
        let first = first_part.first();
        let mut current = first_part.last();
        let mut outer = first_part.nodes;

        loop {
            let mut added_part = false;
            let mut i = 0;
            while i < boundary_parts.len() && current != first {
                if current == boundary_parts[i].first() {
                    // the start of current way touch the polygon, we add it and remove it from the
                    // pool
                    current = boundary_parts[i].last();
                    outer.append(&mut boundary_parts[i].nodes);
                    boundary_parts.remove(i);
                    added_part = true;
                } else if current == boundary_parts[i].last() {
                    // the end of the current way touch the polygon, we reverse the way and add it
                    current = boundary_parts[i].first();
                    boundary_parts[i].nodes.reverse();
                    outer.append(&mut boundary_parts[i].nodes);
                    boundary_parts.remove(i);
                    added_part = true;
                } else {
                    i += 1;
                    // didnt do anything, we want to explore the next way, if we had do something we
                    // will have removed the current way and there will be no need to increment
                }
            }
            if current == first {
                // our polygon is closed, we create it and add it to the multipolygon
                let outer = outer.iter()
                    .map(|n| {
                        Point(Coordinate {
                            x: n.lat(),
                            y: n.lon(),
                        })
                    })
                    .collect();
                multipoly.0.push(Polygon::new(LineString(outer), vec![]));
                break;
            }
            if !added_part {
                use geo::haversine_distance::HaversineDistance;
                let p = |n: &osmpbfreader::Node| {
                    Point(Coordinate {
                        x: n.lat(),
                        y: n.lon(),
                    })
                };
                let distance = p(outer.first().unwrap())
                    .haversine_distance(&p(outer.last().unwrap()));
                if distance < 10. {
                    warn!("boundary: relation/{} ({}): unclosed polygon, dist({:?}, {:?}) = {}",
                          relation.id.0,
                          relation.tags.get("name").map_or("", String::as_str),
                          outer.first().unwrap().id,
                          outer.last().unwrap().id,
                          distance);
                }
                break;
            }
        }
    }
    if multipoly.0.is_empty() {
        None
    } else {
        Some(multipoly)
    }
}

pub fn make_centroid(boundary: &Option<MultiPolygon<f64>>) -> mimir::Coord {
    let coord = boundary.as_ref()
        .and_then(|b| b.centroid().map(|c| mimir::Coord::new(c.x(), c.y())))
        .unwrap_or_else(|| mimir::Coord::default());
    if coord.is_valid() {
        coord
    } else {
        mimir::Coord::default()
    }
}

#[test]
fn test_build_bounadry_empty() {
    let objects = BTreeMap::new();
    let mut relation = osmpbfreader::Relation {
        id: osmpbfreader::RelationId(12),
        refs: vec![],
        tags: osmpbfreader::Tags::new(),
    };
    relation.refs.push(osmpbfreader::Ref {
        member: osmpbfreader::WayId(4).into(),
        role: "outer".to_string(),
    });
    relation.refs.push(osmpbfreader::Ref {
        member: osmpbfreader::WayId(65).into(),
        role: "outer".to_string(),
    });
    relation.refs.push(osmpbfreader::Ref {
        member: osmpbfreader::WayId(22).into(),
        role: "".to_string(),
    });
    assert!(build_boundary(&relation, &objects).is_none());
}

#[test]
fn test_build_bounadry_not_closed() {
    let mut builder = osm_builder::OsmBuilder::new();
    let rel_id = builder.relation()
        .outer(vec![named_node(3.4, 5.2, "start"), named_node(5.4, 5.1, "1")])
        .outer(vec![named_node(5.4, 5.1, "1"), named_node(2.4, 3.1, "2")])
        .outer(vec![named_node(2.4, 3.2, "2"), named_node(6.4, 6.1, "end")])
        .relation_id
        .into();
    if let osmpbfreader::OsmObj::Relation(ref relation) = builder.objects[&rel_id] {
        assert!(build_boundary(&relation, &builder.objects).is_none());
    } else {
        assert!(false); //this should not happen
    }
}

#[test]
fn test_build_bounadry_closed() {
    let mut builder = osm_builder::OsmBuilder::new();
    let rel_id = builder.relation()
        .outer(vec![named_node(3.4, 5.2, "start"), named_node(5.4, 5.1, "1")])
        .outer(vec![named_node(5.4, 5.1, "1"), named_node(2.4, 3.1, "2")])
        .outer(vec![named_node(2.4, 3.2, "2"), named_node(6.4, 6.1, "start")])
        .relation_id
        .into();
    if let osmpbfreader::OsmObj::Relation(ref relation) = builder.objects[&rel_id] {
        let multipolygon = build_boundary(&relation, &builder.objects);
        assert!(multipolygon.is_some());
        let multipolygon = multipolygon.unwrap();
        assert_eq!(multipolygon.0.len(), 1);
    } else {
        assert!(false); //this should not happen
    }
}

#[test]
fn test_build_bounadry_closed_reverse() {
    let mut builder = osm_builder::OsmBuilder::new();
    let rel_id = builder.relation()
        .outer(vec![named_node(2.4, 3.2, "2"), named_node(6.4, 6.1, "start")])
        .outer(vec![named_node(5.4, 5.1, "1"), named_node(2.4, 3.1, "2")])
        .outer(vec![named_node(3.4, 5.2, "start"), named_node(5.4, 5.1, "1")])
        .relation_id
        .into();
    if let osmpbfreader::OsmObj::Relation(ref relation) = builder.objects[&rel_id] {
        let multipolygon = build_boundary(&relation, &builder.objects);
        assert!(multipolygon.is_some());
        let multipolygon = multipolygon.unwrap();
        assert_eq!(multipolygon.0.len(), 1);
    } else {
        assert!(false); //this should not happen
    }
}

#[test]
fn test_build_one_boundary_closed() {
    let mut builder = osm_builder::OsmBuilder::new();
    let rel_id = builder.relation()
        .outer(vec![named_node(3.4, 5.2, "start"),
                    named_node(5.4, 5.1, "1"),
                    named_node(2.4, 3.1, "2"),
                    named_node(6.4, 6.1, "start")])
        .relation_id
        .into();
    if let osmpbfreader::OsmObj::Relation(ref relation) = builder.objects[&rel_id] {
        let multipolygon = build_boundary(&relation, &builder.objects);
        assert!(multipolygon.is_some());
        let multipolygon = multipolygon.unwrap();
        assert_eq!(multipolygon.0.len(), 1);
    } else {
        assert!(false); //this should not happen
    }
}


#[test]
fn test_build_two_opposite_clockwise_boundaries() {
    let mut builder = osm_builder::OsmBuilder::new();
    let rel_id = builder.relation()
        .outer(vec![named_node(0.0, 0.0, "start"), // anti-clockwise polygon
                    named_node(0.0, 1.0, "1"),
                    named_node(1.0, 1.0, "2"),
                    named_node(1.0, 0.0, "3"),
                    named_node(0.0, 0.0, "start")])
        .outer(vec![named_node(0.0, 0.0, "another_start"), // clockwise polygon
                    named_node(0.0, -1.0, "4"),
                    named_node(-1.0, -1.0, "5"),
                    named_node(-1.0, 0.0, "6"),
                    named_node(0.0, 0.0, "another_start")])
        .relation_id
        .into();
    if let osmpbfreader::OsmObj::Relation(ref relation) = builder.objects[&rel_id] {
        let multipolygon = build_boundary(&relation, &builder.objects);
        assert!(multipolygon.is_some());
        let multipolygon = multipolygon.unwrap();
        assert_eq!(multipolygon.0.len(), 2);
        let centroid = make_centroid(&Some(multipolygon));
        assert_eq!(centroid.lon(), 0.0);
        assert_eq!(centroid.lat(), 0.0);
    } else {
        assert!(false); //this should not happen
    }
}

#[test]
fn test_build_two_boundary_closed() {
    let mut builder = osm_builder::OsmBuilder::new();
    let rel_id = builder.relation()
        .outer(vec![named_node(3.4, 5.2, "start"),
                    named_node(5.4, 5.1, "1"),
                    named_node(2.4, 3.1, "2"),
                    named_node(6.4, 6.1, "start")])
        .outer(vec![named_node(13.4, 15.2, "1start"),
                    named_node(15.4, 15.1, "11"),
                    named_node(12.4, 13.1, "12"),
                    named_node(16.4, 16.1, "1start")])
        .relation_id
        .into();
    if let osmpbfreader::OsmObj::Relation(ref relation) = builder.objects[&rel_id] {
        let multipolygon = build_boundary(&relation, &builder.objects);
        assert!(multipolygon.is_some());
        let multipolygon = multipolygon.unwrap();
        assert_eq!(multipolygon.0.len(), 2);
    } else {
        assert!(false); //this should not happen
    }
}
