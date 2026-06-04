// Reference output from rosidl_generator_rs for std_msgs/Bool (RMW layer)
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
