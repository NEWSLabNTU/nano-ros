# nros-orchestration

Runtime support types for generated nano-ros system packages.

This crate intentionally does not parse `nros-plan.json`. Generated package
`build.rs` code reads host-side plan files and emits typed Rust tables that
target code consumes through these no-std types.
