// Reference output from rosidl_generator_rs for std_msgs/String (RMW layer)
// This is a simplified reference for comparison testing

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct String {
    pub data: rosidl_runtime_rs::String,
}

impl Default for String {
    fn default() -> Self {
        String {
            data: Default::default(),
        }
    }
}
