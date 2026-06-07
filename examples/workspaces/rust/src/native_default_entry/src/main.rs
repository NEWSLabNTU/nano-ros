//! Entry pkg — boots the `demo_bringup` topology using the Bringup
//! package's default launch file.
//!
//! This shares the same Node packages and Bringup package as
//! `native_entry`; it exists to exercise the multiple-Entry workspace
//! workflow.

nros::main!(launch = "demo_bringup");
