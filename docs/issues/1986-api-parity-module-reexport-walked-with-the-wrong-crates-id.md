---
id: 1986
title: "api-parity `surface()` walks a cross-crate module re-export with the re-exporting crate's id in the defining crate's index"
status: open
type: bug
area: [testing]
severity: medium
found: 2026-10-19
related: []
---

## Measured

`nros::port_park::install_port_park` appeared in the Rust extraction on one
tree and vanished on the next, with no change to `port_park` itself:

- #2423 (ccf3561101): the gate asked for a ledger row for
  `install_port_park`, because the extraction produced it. The row was added.
- #2424 (77e4175e77): the gate refused the same row with
  `rust:install_port_park names nothing the extraction produced`. The row
  was deleted.

Neither PR touched `port_park` or `scripts/api_parity/`.

## Reading

`packages/api/nros/src/lib.rs` re-exports a module from another crate:

```rust
pub use nros_node::executor::port_park;
```

In `scripts/api_parity/extract_rust.py`, `surface()` follows it like this:

1. `emit(d = nros, item_id = <use target id>)` handles the `use` and calls
   `emit(nros, target)`.
2. That `emit` runs `d, it = docs.resolve(d, item_id)`. `resolve` follows
   the id through `nros`'s `paths` into `nros_node`, so `d` is now the
   `nros_node` doc and `it` is the module item. But `item_id` is still the id
   that `nros` assigned.
3. `kind == "module"` calls `walk_module(d, item_id, qual)`, that is,
   `nros`'s id looked up in `nros_node`'s index.
4. `walk_module` calls `docs.resolve(nros_node, <nros id>)`. Item ids are
   per-crate, so that id points to whatever `nros_node` happens to number
   the same:
   - an unrelated non-module item: the walk returns nothing and the module's
     items drop silently;
   - nothing at all: a `paths` miss, and again nothing;
   - only by coincidence the right module.

The id numbering in both crates shifts with unrelated edits. So the result
flips from tree to tree, which is what #2423 and #2424 saw.

The glob `use` branch is not affected: it passes the use's id with the
re-exporter's doc, and `walk_module` resolves both together. Only the
non-glob module re-export pairs a resolved doc with an unresolved id.

Possibly related, not measured: `emit`'s `seen` key is
`(str(item_id), path)` with no crate in it. So two crates' items with the same
numeric id at the same path dedup against each other.

## What it is NOT

- Not a ledger or gate bug: the gate correctly compares the ledger with what
  the extraction produced. The extraction is what is unstable.
- Not specific to `port_park`. Any `pub use other_crate::module;` in a facade
  is exposed. `port_park` is the one that flipped.

## What would close it

- Walk the module with the id it has in its defining doc. For example,
  `walk_module` takes the resolved `(doc, item)` instead of re-resolving
  `item_id`. Or `resolve` also returns the target-local id, and
  `kind == "module"` uses it.
- Key `seen` by `(crate name, id, path)`.
- Add a regression test: a two-crate fixture where the facade's id for a
  re-exported module names a different item in the defining crate. The test
  asserts the module's items are extracted.
- Re-add the `install_port_park` ledger row once the extraction produces it
  on every tree.
