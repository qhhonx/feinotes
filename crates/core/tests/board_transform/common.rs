//! Shared setup for the transform tests.
#![allow(dead_code)]

use fdl_core::board::math::Point;
use fdl_core::board::mutate::fix_timestamp;
use fdl_core::board::new_element::*;
use fdl_core::board::random::reseed;
use fdl_core::board::types::*;
use fdl_core::board::utils::get_size_from_points;

pub fn setup() { reseed(1); fix_timestamp(Some(1)); }
pub fn close(a: f64, e: f64, digits: i32) { assert!((a - e).abs() < 10f64.powi(-digits) / 2., "{a} not close to {e} ({digits} digits)"); }
pub fn opts(id: &str, x: f64, y: f64, w: f64, h: f64) -> ElementOpts { ElementOpts { id: Some(id.into()), x, y, width: w, height: h, ..Default::default() } }
pub fn shape(ty: ElementType, id: &str, x: f64, y: f64, w: f64, h: f64) -> Element { new_element(ty, opts(id, x, y, w, h)) }
pub fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Element { shape(ElementType::Rectangle, id, x, y, w, h) }
pub fn arrow(id: &str, x: f64, y: f64, points: Vec<Point>) -> Element {
    let (w, h) = get_size_from_points(&points);
    new_arrow_element(opts(id, x, y, w, h), points, None, Some(Arrowhead::Arrow), false, None)
}
pub fn line(id: &str, x: f64, y: f64, points: Vec<Point>) -> Element {
    let (w, h) = get_size_from_points(&points);
    new_linear_element(opts(id, x, y, w, h), points, None)
}
