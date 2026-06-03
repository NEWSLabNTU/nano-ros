//! Phase 212.N.9 entry-poc lib — companion to `src/main.rs`.
//!
//! Exposes the `pub fn register(runtime)` symbol the
//! `nros::main!()` no-arg form emits as
//! `::entry_poc::register(runtime)?;`.
//!
//! The body is **intentionally empty** — this POC verifies the
//! macro emit + the Board boot path reach user code without needing
//! a live zenohd / executor. The actual `register()` symbol is
//! defined by the `nros::node!()` macro below; we provide an
//! `EntryPoc` that satisfies the trait but creates no real
//! subscriptions so `cargo build && ./target/debug/entry-poc`
//! exits 0 with no running RMW broker. Production Entry pkgs would
//! declare real nodes here.

#![no_std]

use nros::{Component, ComponentContext, ComponentResult};

pub struct EntryPoc;

impl Component for EntryPoc {
    const NAME: &'static str = "entry_poc";

    fn register(_ctx: &mut ComponentContext<'_>) -> ComponentResult<()> {
        // No node / publisher / subscription — keeps the POC's
        // exit-0 contract under the no-zenohd CI environment.
        Ok(())
    }
}

nros::declarative_component!(EntryPoc);
nros::node!(EntryPoc);
