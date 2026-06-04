// Reference output from rosidl_generator_rs for geometry_msgs/Point (RMW layer)
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
