---
id: 1873
title: "The registry pin set never drops a range, so a bump leaves its old version pinned — quick-xml 0.37.5 stayed after issue 1805 moved off it for two RUSTSEC advisories"
status: resolved
type: bug
area: [build, tooling, security]
severity: medium
found: 2026-10-13
related: [1805, phase-485]
---

## Measured

Issue 1805 (`3ceff797b8`) moved every locked root from quick-xml 0.37.5 to
0.41.0 because of RUSTSEC-2026-0194/0195, using `nros update --crate quick-xml
--precise 0.41.0`. It also moved indicatif 0.17 → 0.18 to drop the unmaintained
`number_prefix`. Afterwards `locks/cargo-registry.lock` still held quick-xml
0.37.5, indicatif 0.17.11 and number_prefix 0.4.0. No tracked lock resolved
any of them: all 24 locked roots, every tracked `Cargo.lock` and the
`play_launch` submodule's locks resolve quick-xml 0.41.0.

The cause is deliberate. `registry_pins::regenerate` drops nothing: "a range
no root names stays pinned, because a leaf not built here may still resolve
it". So every bump leaves its old range behind as the pin for any leaf that
asks for it. `just check deny` scans only `Cargo.toml` and
`packages/cli/Cargo.toml`, never the pin set, so a vulnerable or unmaintained
pin is never reported.

Of 517 pins, **34** were ranges no locked root resolves. Each was checked
against every leaf lock on the measuring host (862 of them) and against the
pin set's own dependency edges:

- **24 retired.** These are superseded in the roots, or pulled in only by
  something superseded, or removed from the code. No leaf lock written after
  the bump uses them. The 225 leaf locks that still held quick-xml 0.37.5 all
  predate the bump and pair it with the old `nros-pkg-index 0.5.0`, so they
  re-resolve on their next build.
  - quick-xml 0.37 and indicatif 0.17, the two issue 1805 meant to retire,
    plus the crates only indicatif 0.17 pulled in: number_prefix 0.4 and
    console 0.15.
  - anstream 0.6, anstyle-parse 0.2, clap_lex 0.7.
  - pyo3, pyo3-build-config, pyo3-ffi, pyo3-macros and pyo3-macros-backend
    0.22, plus target-lexicon 0.12. memoffset 0.9 and unindent 0.2 were
    pulled in only by pyo3 0.22.
  - env_filter 1: the leaves' env_logger 0.11 resolves env_filter 2.
  - askama 0.12, askama_derive, askama_escape, askama_parser, basic-toml,
    mime, mime_guess and unicase. askama was removed from the code in
    phase-335 W6 (`925b41f899`), and only its pins outlived it.
- **10 kept,** with the leaf that needs each: syn 3, core_detect,
  multiversion_no_op, simdutf8, nom 7, minimal-lexical, env_logger 0.11 and
  env_filter 2, all measured in example leaves. include_dir 0.7 and its
  macros are kept but marked UNVERIFIED: no root ever used them, and no leaf
  on the measuring host does.

## Fix

- **`locks/leaf-only-pins.toml`:** the keep list. Each range that no locked
  root resolves is named with why.
- **`check-cargo-registry-pins`** (fast line) now also fails on:
  - a pinned range that no locked root resolves and the list does not keep;
  - a keep entry that is no longer pinned, or that a root resolves anyway.

  The new checks are in its self-test.
- **`nros update --pins --prune`** drops the unclaimed ranges
  (`registry_pins::prune`, unit-tested). It refuses, rather than half-doing
  it, when a kept pin depends on a dropped one.
- **Ran it:** 24 ranges retired, `locks/cargo-registry.lock` 517 → 493 pins,
  no root lock moved, and no remaining pin depends on a retired one. A fresh
  seed of `examples/px4/rust/companion/px4-stub` from the pruned set
  resolves (`cargo metadata`), at the kept env_logger 0.11.11 and env_filter
  2.0.0.

## Not covered

`just check deny` still reads only the repo-root and CLI workspaces. A
range that a root resolves is held to the pin set, and a range no root
resolves now needs a written reason. Neither is an advisory check of the
pin set itself.
