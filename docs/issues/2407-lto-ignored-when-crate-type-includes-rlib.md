---
id: 2407
title: "`lto` in the release profile does nothing for nros-cpp or a generated Zephyr Rust entry: cargo drops LTO for a crate whose crate-type includes `lib`/`rlib`, and a nucleo image is 60,772 B (16 %) larger in .text than with it"
status: open
type: bug
severity: medium
area: [build, cargo, memory, zephyr, cli]
found: 2026-11-10
related: [issue-2400, phase-548]
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**. It was
found while measuring the nucleo-f767zi image for issue 2400.

## What

The root release profile says `lto = "fat"`, and a generated Zephyr Rust entry
says `lto = true`. Neither one takes effect on the crate that produces the
image's archive. cargo passes no `-C lto` to a crate whose `crate-type` also
lists `lib` or `rlib`, and it gives no warning. Both crates in question list
one:

- `packages/api/nros-cpp/Cargo.toml`: `crate-type = ["staticlib", "lib"]`
- the entry template `nros-cli-core/src/cmd/new_entry.rs`:
  `crate-type = ["staticlib", "rlib"]`, so every entry `nros new` writes has
  it
- `nros-c` (`["staticlib", "cdylib", "lib"]`) and the
  `nros-tests/fixtures/multi_pkg_workspace_*` packages have the same shape.
  This issue does not measure them.

`cargo build -v` shows it directly. The as-is `nros-cpp` build passes
`--crate-type staticlib --crate-type lib` and no `-C lto`.
`cargo rustc --crate-type staticlib` on the same crate passes `-C lto=fat`.

## Measured (2026-11-10, Rust 1.99.0)

| image | as-is | staticlib only (LTO applied) | delta |
| --- | ---: | ---: | ---: |
| `nros-cpp` archive, thumbv7em release, features `alloc,log,panic-platform,rmw-cffi,platform-freertos`: `.text` summed over members | 985,938 B (532 members) | 644,812 B (505 members) | −341,126 B (−35 %) |
| **linked** nucleo-f767zi Zephyr image, `nros new --lang rust --rmw zenoh --template action-server`: `zephyr.elf` `.text` | 375,560 B | 314,788 B | **−60,772 B (−16 %)** |
| same image, FLASH as the Zephyr link reports it | 380,176 B | 319,392 B | −60,784 B |
| same image, RAM | 281,856 B | 280,640 B | −1,216 B |

The archive figure overstates the saving, because the linker's
`--gc-sections` drops unreferenced code anyway. The linked image is the real
number. The LTO image was only built, never run. The change was made in a
scratch project's generated `zephyr_entry/Cargo.toml` and rebuilt with `ninja`
alone.

## Why it matters

- Every Rust Zephyr image `nros new` generates misses the size win its own
  profile claims. On a 2 MB part that is 16 % of `.text`.
- Stack frames change under LTO, because inlining moves code between frames.
  Issue 2400's floors are measured on the images as they ship today. If this
  is fixed, every entry in `config/stack-floors.toml` has to be re-walked.
  The table's STALE check reports it when that happens.

## Where to look

Why the `lib`/`rlib` crate-type is there in each case. In nros-cpp it is the
Rust API that other crates depend on. In the entry template no other crate
depends on `rustapp`, as far as a reader can tell. Possible fixes: drop the
`rlib` where nothing depends on it, or build the archive through
`cargo rustc --crate-type staticlib`. Either way, the PR must include a
measured image that runs.
