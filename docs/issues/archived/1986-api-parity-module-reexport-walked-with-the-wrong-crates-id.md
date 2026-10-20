---
id: 1986
title: "api-parity `surface()` walks a cross-crate module re-export with the re-exporting crate's id in the defining crate's index"
status: resolved
type: bug
area: [testing]
severity: medium
found: 2026-10-19
related: []
resolved_in: 2026-10-20
---

## What was wrong

`nros::port_park::install_port_park` appeared in the Rust extraction on one
tree (#2423, row added) and vanished on the next (#2424, row deleted), with
no change to `port_park`.

In `scripts/api_parity/extract_rust.py` `surface()`, a non-glob
`pub use other_crate::module;` resolved the use target into the defining
crate's doc, then walked the module with the RE-EXPORTER's id in that doc.
Item ids are per-crate, so the walk found an unrelated item, nothing, or the
right module by coincidence, and the answer moved with unrelated edits.

The `seen` key `(id, path)` had the same flaw: a glob re-export emits the
other crate's children at the facade's own path, so an equal id in two
crates dropped one of the two items.

## Fix

- The module branch walks the item it already resolved (`walk_items(d, it,
  qual)`), never re-resolving the re-exporter's id.
- `seen` is keyed by `(crate, id, path)`.
- `extract_rust.self_test()` (run by `api-parity --self-test`) walks a
  synthetic two-crate rustdoc JSON whose ids collide on purpose. On the old
  code it reports both losses (the re-exported module's function and the
  facade's own function); on the new code it passes.

## What the fix newly extracted

48 qualified names (49 records), none dropped:

- `nros::port_park::install_port_park`: its `rust:install_port_park` row in
  `other.json` is back.
- 47 names, 48 records, under `nros::heapless` (`pub use
  nros_core::heapless;`, a crates.io crate; `Iter` is declared twice). They are heapless's API, not ours, so
  `extract_rust.dependency_records` excludes records declared under a cargo
  `registry/src` or `git/checkouts` tree from OUR surface, and the run prints
  how many it cut.
