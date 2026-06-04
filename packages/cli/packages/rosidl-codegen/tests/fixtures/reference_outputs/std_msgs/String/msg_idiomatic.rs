// Reference output from rosidl_generator_rs for std_msgs/String (idiomatic layer)
// This is a simplified reference for comparison testing

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct String {
    pub data: std::string::String,
}

impl Default for String {
    fn default() -> Self {
        String {
            data: Default::default(),
        }
    }
}

impl From<crate::msg::rmw::String> for String {
    fn from(rmw: crate::msg::rmw::String) -> Self {
        String {
            data: rmw.data.to_string(),
        }
    }
}

impl From<String> for crate::msg::rmw::String {
    fn from(idiomatic: String) -> Self {
        crate::msg::rmw::String {
            data: idiomatic.data.into(),
        }
    }
}
