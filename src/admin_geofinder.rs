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
use mimir::Admin;
use geo::contains::Contains;
use geo;
use std::iter::FromIterator;
use std::rc::Rc;
use gst::rtree::{RTree, Rect};
use std;

/// We want to strip the admin's boundary for the objects referencing it (for performance purpose)
/// thus in the `AdminGeoFinder` we store an Admin without the boundary (the option is emptied)
/// and we store the boundary aside
struct BoundaryAndAdmin(Option<geo::MultiPolygon<f64>>, Rc<Admin>);

impl BoundaryAndAdmin {
    fn new(mut admin: Admin) -> BoundaryAndAdmin {
        let b = std::mem::replace(&mut admin.boundary, None);
        let minimal_admin = Rc::new(admin);
        BoundaryAndAdmin(b, minimal_admin)
    }
}


pub struct AdminGeoFinder {
    admins: RTree<BoundaryAndAdmin>,
}

impl AdminGeoFinder {
    pub fn insert(&mut self, admin: Admin) {
        use ordered_float::OrderedFloat;
        fn min(a: OrderedFloat<f32>, b: f64) -> f32 {
            a.0.min(down(b as f32))
        }
        fn max(a: OrderedFloat<f32>, b: f64) -> f32 {
            a.0.max(up(b as f32))
        }

        let rect = {
            let mut coords = match admin.boundary {
                Some(ref b) => (*b).0.iter().flat_map(|poly| (poly.exterior).0.iter()),
                None => return,
            };
            let first_coord = match coords.next() {
                Some(c) => c,
                None => return,
            };
            let first_rect: Rect = {
                let (x, y) = (first_coord.x() as f32, first_coord.y() as f32);
                Rect::from_float(down(x), up(x), down(y), up(y))
            };
            coords.fold(first_rect, |accu, p| {
                Rect::from_float(min(accu.xmin, p.x()),
                                 max(accu.xmax, p.x()),
                                 min(accu.ymin, p.y()),
                                 max(accu.ymax, p.y()))
            })
        };
        self.admins.insert(rect, BoundaryAndAdmin::new(admin));
    }

    /// Get all Admins overlapping the coordinate
    pub fn get(&self, coord: &geo::Coordinate<f64>) -> Vec<Rc<Admin>> {
        let (x, y) = (coord.x as f32, coord.y as f32);
        let search = Rect::from_float(down(x), up(x), down(y), up(y));
        self.admins
            .get(&search)
            .into_iter()
            .map(|(_, a)| a)
            .filter(|a| a.0.as_ref().map_or(false, |b| (*b).contains(&geo::Point(*coord))))
            .map(|admin_and_boundary| admin_and_boundary.1.clone())
            .collect()
    }

    /// Iterates on all the admins with a not None boundary.
    pub fn admins<'a>(&'a self) -> Box<Iterator<Item = Admin> + 'a> {
        let iter = self.admins
            .get(&Rect::from_float(std::f32::NEG_INFINITY,
                                   std::f32::INFINITY,
                                   std::f32::NEG_INFINITY,
                                   std::f32::INFINITY))
            .into_iter()
            .map(|(_, a)| {
                let mut admin = (*a.1).clone();
                admin.boundary = a.0.clone();
                admin
            });
        Box::new(iter)
    }

    /// Iterates on all the `Rc<Admin>` in the structure as returned by `get`.
    pub fn admins_without_boundary<'a>(&'a self) -> Box<Iterator<Item = Rc<Admin>> + 'a> {
        let iter = self.admins
            .get(&Rect::from_float(std::f32::NEG_INFINITY,
                                   std::f32::INFINITY,
                                   std::f32::NEG_INFINITY,
                                   std::f32::INFINITY))
            .into_iter()
            .map(|(_, a)| a.1.clone());
        Box::new(iter)
    }
}

impl Default for AdminGeoFinder {
    fn default() -> Self {
        AdminGeoFinder { admins: RTree::new() }
    }
}

impl FromIterator<Admin> for AdminGeoFinder {
    fn from_iter<I: IntoIterator<Item = Admin>>(admins: I) -> Self {
        let mut geofinder = AdminGeoFinder::default();

        for admin in admins {
            geofinder.insert(admin);
        }

        geofinder
    }
}

// the goal is that f in [down(f as f32) as f64, up(f as f32) as f64]
fn down(f: f32) -> f32 {
    f - (f * ::std::f32::EPSILON).abs()
}
fn up(f: f32) -> f32 {
    f + (f * ::std::f32::EPSILON).abs()
}

#[test]
fn test_up_down() {
    for &f in [1.0f64, 0., -0., -1., 0.1, -0.1, 0.9, -0.9, 42., -42.].iter() {
        let small_f = f as f32;
        assert!(down(small_f) as f64 <= f,
                format!("{} <= {}", down(small_f) as f64, f));
        assert!(f <= up(small_f) as f64,
                format!("{} <= {}", f, up(small_f) as f64));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> ::geo::Point<f64> {
        ::geo::Point(::geo::Coordinate { x: x, y: y })
    }

    fn make_admin(offset: f64) -> ::mimir::Admin {
        // the boundary is a big octogon
        let shape = ::geo::Polygon::new(::geo::LineString(vec![p(3. + offset, 0. + offset),
                                                               p(6. + offset, 0. + offset),
                                                               p(9. + offset, 3. + offset),
                                                               p(9. + offset, 6. + offset),
                                                               p(6. + offset, 9. + offset),
                                                               p(3. + offset, 9. + offset),
                                                               p(0. + offset, 6. + offset),
                                                               p(0. + offset, 3. + offset),
                                                               p(3. + offset, 0. + offset)]),
                                        vec![]);
        let boundary = ::geo::MultiPolygon(vec![shape]);

        ::mimir::Admin {
            id: format!("admin:offset:{}", offset),
            level: 8,
            name: "city".to_string(),
            label: format!("city {}", offset),
            zip_codes: vec!["421337".to_string()],
            weight: ::std::cell::Cell::new(0.),
            coord: ::mimir::Coord::new(4.0 + offset, 4.0 + offset),
            boundary: Some(boundary),
            insee: "outlook".to_string(),
        }
    }

    #[test]
    fn test_two_fake_admins() {
        let mut finder = AdminGeoFinder::default();
        finder.insert(make_admin(40.));
        finder.insert(make_admin(43.));

        // outside
        for coord in [p(48., 41.), p(411., 41.), p(51., 54.), p(53., 53.)].iter() {
            assert!(finder.get(&coord.0).is_empty());
        }

        // inside one
        let admins = finder.get(&p(44., 44.).0);
        assert_eq!(admins.len(), 1);
        assert_eq!(admins[0].id, "admin:offset:40");
        let admins = finder.get(&p(48., 48.).0);
        assert_eq!(admins.len(), 1);
        assert_eq!(admins[0].id, "admin:offset:43");

        // inside two
        let mut admins = finder.get(&p(46., 46.).0);
        admins.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(admins.len(), 2);
        assert_eq!(admins[0].id, "admin:offset:40");
        assert_eq!(admins[1].id, "admin:offset:43");
    }
}
