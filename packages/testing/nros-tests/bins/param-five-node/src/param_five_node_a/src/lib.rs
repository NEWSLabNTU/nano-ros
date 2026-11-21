//! Issue 2812 -- `param_a`, one of five nodes that each serve the parameter
//! services.
//!
//! Every node carrying the parameter family runs six servers, and each server
//! owns a request/reply buffer pair on the platform allocator: the ThreadX byte
//! pool, or FreeRTOS heap_4, each of which also hands out the task stacks. Both
//! defaults were measured on one-node images, so the five packages of this
//! fixture are the case they never priced. The node itself is in
//! `common/param_node.rs`, shared by all five.
//!
//! The board prints its own allocator peak (`nros: byte pool peak` on ThreadX,
//! `nros: heap peak` on FreeRTOS), which is what the per-node term is measured
//! from.

#![no_std]

include!("../../../common/param_node.rs");

param_node!(ParamA, "param_a", "/chatter", true);

nros::node!(ParamA);
