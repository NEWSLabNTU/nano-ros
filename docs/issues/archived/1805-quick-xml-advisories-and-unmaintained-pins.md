---
id: 1805
title: "The pin set carries quick-xml 0.37.5 (RUSTSEC-2026-0194/0195, fixed only in 0.41) and five unmaintained crates — found by the first `just check deny`"
status: resolved
type: tech-debt
area: [deps, cli]
severity: medium
found: 2026-10-11
related: [phase-485, rfc-0104]
---

## Measured

The first run of the cargo-deny lane (phase-485 W9, `just check deny`, over
the repo root and the CLI workspace) reported three vulnerabilities and five
unmaintained crates in the registry pin set.

- **crossbeam-epoch 0.9.18** (RUSTSEC-2026-0204, invalid pointer
  dereference in `fmt::Pointer`). The fix is semver-compatible, so it was
  moved in the same change with `nros update --crate crossbeam-epoch`, to
  0.9.21 in both roots and the pin set. It is no longer reported.
- **quick-xml 0.37.5**, still open. RUSTSEC-2026-0194 is quadratic duplicate
  attribute checking; RUSTSEC-2026-0195 is unbounded namespace-declaration
  allocation in `NsReader`. Both are fixed only in 0.41, a BREAKING range from
  0.37, so the fix starts with a manifest edit. The users are
  `nros-pkg-index`, which `nros-macros` reaches, so every Rust user build
  compiles it, and `cargo-nano-ros`.
  - Exposure: both parse `package.xml` files from the host's ament install.
    That is trusted input, so the denial-of-service paths need a hostile
    `package.xml`.
- **Unmaintained, no safe upgrade:**
  - `paste`, reached through `nros`;
  - `proc-macro-error2`, reached through the stm32f4 reference roots' graph;
  - `bare-metal`, reached through `cortex-m`;
  - `term_size`, reached through `tokei`;
  - `number_prefix`, reached through `indicatif` (CLI).

## Fix

- quick-xml: edit the two manifests to `0.41` (`cargo upgrade --incompatible
  -p quick-xml` in `packages/cli`, and by hand in `nros-pkg-index`), adapt
  the call sites, then run `nros update --crate quick-xml --precise
  0.41.x`.
- The unmaintained crates: each is a decision about its parent crate, not a
  pin move. Record the decision here, or replace the parent.

The seven advisory ids open here (RUSTSEC-2026-0194, -0195, -0110,
-2025-0119, -2024-0436, -2026-0173, -2020-0163) are listed in `deny.toml`
`[advisories] ignore`, each naming this issue, so `just check deny` stays red
only for something new. Delete each entry with its fix.

## Fixed

- **quick-xml 0.37.5 -> 0.41.0** in all four manifests (`nros-pkg-index`,
  `cargo-nano-ros`, `nros-launch-parser`, `nros-cli-core`), then `nros update
  --crate quick-xml --precise 0.41.0`. Every locked root moved together.
  RUSTSEC-2026-0194/0195 are no longer reported.
  - The breaking part is semantic, not just a rename. Since 0.38 the reader
    stops at every `&` and reports the reference as its own `GeneralRef`
    event, so `a &amp; b` arrives as three events and `BytesText::unescape`
    is gone. A port that only swapped `unescape` for `decode` would quietly
    keep `"a "` as the whole value. `nros_pkg_index::xml` is now the one
    spelling: `ElementText` collects `Text` + `GeneralRef` until the element
    ends (trimming the whole, so `trim_text(true)` is off in both readers), and
    `attr_value` uses `normalized_value` in place of the deprecated
    `unescape_value`. Both `package.xml` readers and both attribute readers
    use it.
- **number_prefix**: indicatif 0.17 -> 0.18 (`cargo-nano-ros`), which uses
  `unit-prefix` instead. Gone from the CLI root.
- **paste**: nano-ros's own use (`nros`, `nros-c`) moved to `pastey` 0.2, the
  maintained drop-in fork. `paste` stays in the graph through `riscv` 0.15 and
  `esp-hal` 1.0.

Decided, and kept in `deny.toml` `[advisories] ignore` with the parent named:

- `bare-metal`: held by `cortex-m` 0.7, which is the current release line.
- `paste`: held by `riscv` and `esp-hal`, as above.
- `proc-macro-error2`: held by `defmt-macros` 1.1.
- `term_size`: held by `tokei`. tokei 15.0.0 still depends on it
  unconditionally (measured), so moving the `nros-tests` dev-dependency
  would buy nothing.

`just check deny` is green, and each remaining ignore is hit by at least one
root.

The pin set still lists quick-xml 0.37.5 next to 0.41.0. `nros update` never
drops a range, because a leaf not built here may still resolve it. No in-tree
manifest asks for 0.37 any more, so no seeded lock picks it up.
