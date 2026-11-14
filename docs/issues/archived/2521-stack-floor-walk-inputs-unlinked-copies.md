---
id: 2521
title: "The stack walk reads unlinked copies of a crate's bitcode, so the same template walks differently per build"
status: resolved
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

## Resolution (2026-11-14, phase-557 W7c)

The walk now selects the bitcode the link used:

- `linked_codegen_units` reads every Rust codegen unit (`<unit>.rcgu.o`) a
  GNU ld or LLD map names, as an archive member or a loose object.
- `select_linked_bitcode` keeps a `--bitcode-dir` module only when its unit
  is one of them, and walks a unit found twice with the same bytes once. A
  fat-LTO dir's merged module is always kept (the map does not name it by
  unit). With no map, nothing changes.
- `--sysroot` takes the core / alloc / compiler_builtins rlibs the link used,
  matched by crate hash across the installed toolchains, and only for crates
  `--bitcode-dir` does not already supply (build-std). This found that a
  fresh Zephyr Rust project links the `stable` toolchain's sysroot, not the
  pinned one (issue 2546), so the walk had been reading the pinned
  toolchain's bitcode for code the image did not contain.

**Measured.** Three `nros new` projects per nucleo template, at three paths
(so three sets of crate hashes), each walk to identical bytes:

| template | before | after |
| --- | ---: | ---: |
| Rust action server | 76,624 | 76,000 |
| Rust talker | 72,528 | 72,240 |
| C talker | 127,852 / 127,948 | 104,180 |

The mps2 bare-metal talker (cargo road) is unchanged at 90,200: 415 unlinked
units are left out, none on its path.
