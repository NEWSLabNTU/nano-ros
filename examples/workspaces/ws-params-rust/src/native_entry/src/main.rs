//! Entry pkg — boots the parameterised system on the native board.
//!
//! `nros::main!()` reads the launch file at expansion time and **compile-bakes** each
//! `<param name=… value=…/>` into the generated entry, setting `runtime.params` before
//! the node `register` call (phase-264 W4a, RFC-0004 §10). `ParamTalker::register`
//! reads the value via `ctx.param("publish_period_ms")` and sets its timer period —
//! so the launch file configures the node with no per-app glue and no extra `nros`
//! feature.

nros::main!(model = "demo_bringup");
