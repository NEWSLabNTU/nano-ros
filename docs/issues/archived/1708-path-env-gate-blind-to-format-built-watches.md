---
id: 1708
title: "`check-path-env-fingerprints` read only `cargo:rerun-if-env-changed=NAME`
  LITERALS, so a path watch spelled through a const, a `format!`, or the
  `cargo::` prefix passed — issue 1623's deliberate one included, unrecorded"
status: resolved
type: tech-debt
area: [build, ci]
severity: medium
found: 2026-10-06
related: [issue-0491, issue-1623, issue-1220, phase-472]
resolved_in: "branch fix/path-env-gate-format"
---

## What

`check-path-env-fingerprints` enforces issue 0491: never
`cargo:rerun-if-env-changed` on a variable that NAMES a path (cargo compares
it as text, and one directory has several spellings here), watch the content
with `rerun-if-changed` instead. Its Rust producer was one regex,
`cargo:rerun-if-env-changed=([A-Za-z_]…)`, over raw text. That left three
spellings it could not see:

1. **a name built from a constant** — `println!("…={}", SOME_ENV)`,
   `format!("…={SOME_ENV}")`, named arguments. Interpolated names were skipped
   on the premise that they are all knob tables;
2. **the `cargo::` prefix** (cargo 1.77+), even as a plain literal — the regex
   needs `cargo:` followed directly by `rerun`;
3. consequently, issue 1623's site:
   `nros_sizing_descriptor::from_env_value_emitting` emits
   `format!("cargo::rerun-if-env-changed={DESCRIPTOR_ENV}")`, a path variable
   watched by name ON PURPOSE. Being both const-built and `cargo::`-spelled,
   the gate never saw it, so the exception was nowhere recorded, and an
   undeliberate second one would have passed exactly the same way.

This is phase-472's W3/W6 class (evidence the scanner cannot read): the rule
is about every directive, the scan read one spelling of it.

## Reproduction (on origin/main 52dcf5b5a2)

Scratch edit to `packages/rmw/zenoh/nros-rmw-zenoh/build.rs`, confirmed with
`git diff`:

```rust
const SCRATCH_BOARD_ENV: &str = "NROS_BOARD_TOML";
fn main() {
    println!("cargo:rerun-if-env-changed={}", SCRATCH_BOARD_ENV);
```

`python3 scripts/check-path-env-fingerprints.py` → `OK`, rc=0.
Replacing that line with the literal
`println!("cargo::rerun-if-env-changed=NROS_BOARD_TOML");` → also `OK`, rc=0.

## Fix

Producer 1 now reads every directive STRING (comments stripped by
`scripts/lib/comments.py`; `#[cfg(test)]` items skipped via
`per_item.rust_cfg_test_blank`, since a test emits nothing to cargo), in both
`cargo:` and `cargo::` spellings, and resolves a `{}` / `{N}` / `{name}` /
`{CONST}` placeholder through the macro's arguments to a
`const NAME: &str = "…"` — this crate first, then tree-wide when unambiguous.
The resolved name goes through the same classifier as a literal.

- **Fails closed**: an argument that is not such a const (a parameter, a loop
  variable, a field, a call, a partial name like `NROS_{x}_DIR`) is a failure
  unless an `UNRESOLVED_EXEMPTIONS` row keyed on (file, enclosing fn,
  expression) says where every caller's name comes from. 14 rows, one per
  existing helper/loop. All of them are knob readers or the platform-manifest
  loop that producer 2 already classifies. An exemption row matching no site
  is STALE and fails.
- **1623's exception is explicit**: `WATCH_EXEMPTIONS` (an
  `exemptions.Exemptions` table) row
  `("packages/tooling/nros-sizing-descriptor", "NROS_SIZING_DESCRIPTOR")`. The
  reason: the unset↔set edge is needed (issue 1623), and the content is ALSO
  watched by `rerun-if-changed`. `NROS_SIZING_DESCRIPTOR` joins a
  `PATH_NAMES` list, because no suffix says it names a file. Neighbour rows
  (same crate + `NROS_BOARD_TOML`; `nros-node` + `NROS_SIZING_DESCRIPTOR`) are
  proved NOT covered, both in the self-test and through `Exemptions.check`.
- `--list-env-names` (the shadow fixture cache's witness set) gains the three
  names the literal scan missed: `NROS_BOARD_FRAMEWORK` (a `cargo::` literal),
  `NROS_CC_STRICT_DECLS` (`nros-cc-flags`'s `DISABLE_ENV` const) and
  `NROS_SIZING_DESCRIPTOR`.

## Measured (mutation)

| mutation | old gate | new gate |
| --- | --- | --- |
| scratch `"…={}", SCRATCH_BOARD_ENV` (const = `NROS_BOARD_TOML`) | rc=0 | rc=1, names `nros-rmw-zenoh/build.rs:6 NROS_BOARD_TOML (via SCRATCH_BOARD_ENV)` |
| scratch `cargo::rerun-if-env-changed=NROS_BOARD_TOML` literal | rc=0 | rc=1 |
| scratch `"…={}", scratch_name()` (unresolvable) | rc=0 | rc=1, fails closed |
| delete the 1623 `WATCH_EXEMPTIONS` row | — | rc=1, names `nros-sizing-descriptor/src/lib.rs:676 NROS_SIZING_DESCRIPTOR (via DESCRIPTOR_ENV)` |
| untouched tree | rc=0 | rc=0, `115 rerun-if-env-changed site(s) examined` |

## Not done

The unresolved rows exempt a HELPER (`env_usize(name)`). A caller that passes
a path name to such a helper is still not seen. Closing that means harvesting
each helper's call-site arguments, and it is left open.
