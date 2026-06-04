// Reference output from rosidl_generator_rs for std_msgs/Bool (idiomatic layer)
// This is a simplified reference for comparison testing

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bool {
    pub data: bool,
}

impl Default for Bool {
    fn default() -> Self {
        Bool {
            data: Default::default(),
        }
    }
}

impl From<crate::msg::rmw::Bool> for Bool {
    fn from(rmw: crate::msg::rmw::Bool) -> Self {
        Bool { data: rmw.data }
    }
}

impl From<Bool> for crate::msg::rmw::Bool {
    fn from(idiomatic: Bool) -> Self {
        crate::msg::rmw::Bool { data: idiomatic.data }
    }
}
