//! Issue 2812 -- `param_e`, one of five nodes that each serve the parameter
//! services.
//!
//! Every node carrying the parameter family runs six servers, and each server
//! owns a request/reply buffer pair on the platform allocator: the ThreadX byte
//! pool, or FreeRTOS heap_4, each of which also hands out the task stacks. Both
//! defaults were measured on one-node images, so the five packages of this
//! fixture are the case they never priced. The node itself is in
//! `common/param_node.rs`, shared by all five.

#![no_std]

include!("../../../common/param_node.rs");

param_node!(ParamE, "param_e", "/chatter_e", false);

nros::node!(ParamE);
