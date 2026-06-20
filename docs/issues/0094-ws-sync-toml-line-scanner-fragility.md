---
id: 94
title: "`nros ws sync` line-based TOML editor breaks on quoted patch header + explicit dep-tables"
status: open
type: bug
area: build
related: [phase-210, phase-220, phase-244]
---

## Summary

`nros ws sync` rewrites a consumer `Cargo.toml`'s `[patch.crates-io]` block with a
**line-based scanner, not a real TOML parser** (`packages/cli/nros-cli-core/src/cmd/ws.rs`,
`splice_patch_block` / `extract_patch_table` / `extract_consumer_registry_nros_deps`).
For the canonical shape sync itself emits (bare `[patch.crates-io]` header, inline
`name = { version = "*" }` deps, clean relative paths) it is correct, idempotent, and
race-safe (atomic rename, `ws.rs:900-922`). It breaks on TOML-equivalent shapes a human
or a TOML-aware tool can legitimately produce.

## Cases

### A — HIGH: quoted patch header → duplicate table → cargo hard-errors

`extract_patch_table` locates the existing table via
`line.trim_start().starts_with("[patch.crates-io]")` (`ws.rs:1307`). Only the **bare**
form matches. The TOML-equivalent quoted form `[patch."crates-io"]` (the old
`config_patcher.rs:288` comment already notes both forms occur) is **not** detected →
the splicer believes no table exists → emits a **second** `[patch.crates-io]` header
(`ws.rs:1244`). cargo rejects a manifest with two `[patch.crates-io]` tables. Result:
corrupted manifest, build dead.

### B — MEDIUM-HIGH: explicit dependency tables not scanned → missing patch → unresolved

`extract_consumer_registry_nros_deps` only walks inline `[dependencies]` tables
(`is_dependencies_table`, `ws.rs:1063-1076`). The explicit dotted-table form

```toml
[dependencies.nros]
version = "*"
```

(and `[target.<cfg>.dependencies.<name>]`) is classified as a non-dep section →
`in_deps = false` → the `version = "*"` line is skipped → `nros` never gets a
`[patch.crates-io]` path entry → `cargo` fails post-sync with `no matching package`.
Not corruption, but a silent broken build.

### C — MEDIUM: doubled managed block is sticky

`strip_managed_block` (`ws.rs:1263-1290`) removes only the **first** `BEGIN..END`
region. A prior crash / concurrent writer leaving two blocks is never self-healed:
each re-sync keeps one stale block, eventually duplicating entries / headers.

### D/E/F — LOW (documented, not fixed here)

- Line scanner can be fooled by `[patch.crates-io]` / `[dependencies]` text inside a
  multi-line string (`"""..."""`).
- Emitted `path = "{rel}"` is not TOML-escaped — a workspace path containing `"` or `\`
  (Windows / quirky checkout dir) breaks. Near-zero on Linux.
- Empty managed set still emits a bare `[patch.crates-io]` header (cosmetic).

## Fix direction

A real `toml_edit`-based rewrite would eliminate all six at once and is the long-term
target. This issue's immediate fix hardens the line scanner for A/B/C (the cases that
actually break builds): normalize the patch-header match across bare/quoted forms,
recognize explicit `[dependencies.<name>]` / `[target.*.dependencies.<name>]` tables,
and strip **all** managed blocks. D/E/F left as documented low-risk follow-ups.

## Evidence

Found in a review of `ws.rs` on 2026-06-20 prompted by uncommitted `nros ws sync` regen
drift across `examples/native/rust/*` + `packages/testing/nros-bench/*` Cargo.toml.
