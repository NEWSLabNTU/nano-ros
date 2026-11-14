---
id: 2521
title: "The stack walk reads unlinked copies of a crate's bitcode, so the same template walks differently per build"
status: open
type: bug
severity: low
area: [memory, tooling]
related: [issue-2402, issue-2400, phase-557]
found: 2026-11-14
---

## What

The cmake road gives the walker every `--bitcode-dir` under the build
(`rust/target/*/deps`, `nros-rust*/*/deps`). A C image's build holds three
builds of `nros_c` there: the staticlib, a hashed rlib dependency, and the
C++ lane's copy. Only one of each function is linked. The walk reads all of
them, and under `--indirect bind` an unlinked copy's functions are bind
targets with real frames.

Crate disambiguators (`Cs…`) depend on the project's path, so which unlinked
copy a site's type tier happens to reach differs between two projects made
from the same template.

## Measured

Two `nros new` nucleo-f767zi C talkers, built from the same tree, walk
127,852 B and 127,948 B. The extra 96 B is a path from one copy's
`nros_c::executor::result_trampoline` to another copy's. Neither copy is in
`zephyr.elf`.

`nros new` stamps the template's stack from one recorded walk, so a project
whose walk lands above that record fails its build by a few bytes.

## Not the fix

Treating a framed function absent from the ELF as a 0-byte leaf is unsound
here: linked `#[no_mangle]` functions (`rclc_cdr_write_*`) are defined by
every copy, so their merged edges point into unlinked copies' callees.

## Fix

Select the bitcode the link actually used: map each linked object (the
`--link-map` LOAD lines) back to its `.bc`, and walk only those. Then two
projects from one template walk the same graph up to symbol names.

## Acceptance

Two `nros new` projects from one template walk to the same depth.
