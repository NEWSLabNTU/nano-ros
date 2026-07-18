//! Entry pkg — boots the advanced-launch showcase on the native board.
//!
//! `nros::main!()` resolves `demo_bringup`'s default launch
//! (`system.launch.xml`) at build time — expanding its `<arg>` / `$(var)` /
//! `<group>` / `<remap>` / `<param>` / `<include>` into a flat node list — and
//! emits one `<node_pkg>::register` per resolved node (talker + the listener
//! pulled in from the included sub-launch). Run
//! `nros plan demo_bringup src/demo_bringup/launch/system.launch.xml` to inspect
//! the resolved launch record (args → `alpha`, remap + param recorded) and the
//! lowered orchestration plan.

nros::main!(model = "demo_bringup");
