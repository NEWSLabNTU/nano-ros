// Reference output from rosidl_generator_rs for geometry_msgs/Point (idiomatic layer)
// This is a simplified reference for comparison testing

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Default for Point {
    fn default() -> Self {
        Point {
            x: Default::default(),
            y: Default::default(),
            z: Default::default(),
        }
    }
}

impl From<crate::msg::rmw::Point> for Point {
    fn from(rmw: crate::msg::rmw::Point) -> Self {
        Point {
            x: rmw.x,
            y: rmw.y,
            z: rmw.z,
        }
    }
}

impl From<Point> for crate::msg::rmw::Point {
    fn from(idiomatic: Point) -> Self {
        crate::msg::rmw::Point {
            x: idiomatic.x,
            y: idiomatic.y,
            z: idiomatic.z,
        }
    }
}
