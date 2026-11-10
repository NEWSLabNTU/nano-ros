---
id: 2407
title: "Enable LTO for shipped images without losing size inspection: cargo drops `lto` for a crate whose crate-type includes `lib`/`rlib`, and a nucleo image is 60,772 B (16 %) larger in .text without it"
status: open
type: enhancement
severity: medium
area: [build, cargo, memory, zephyr, cli]
found: 2026-11-10
related: [issue-2400, issue-0815, phase-548]
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**. Found
while measuring the nucleo-f767zi image for issue 2400.

## What

The release profiles ask for LTO: the root says `lto = "fat"`, and a generated
Zephyr Rust entry says `lto = true`. The crate that produces the image's
archive never gets it. cargo passes no `-C lto` to a crate whose `crate-type`
also lists `lib` or `rlib`, and it does not warn. Two such crates:

- `packages/api/nros-cpp/Cargo.toml`: `crate-type = ["staticlib", "lib"]`.
  The `lib` is a real dependency: the mps2 C++ examples, `nros-nuttx-ffi`,
  `nros-nuttx-riscv-ffi` and `heap-free-cpp-poc-mps2` depend on it.
- The entry template in `nros-cli-core/src/cmd/new_entry.rs`:
  `crate-type = ["staticlib", "rlib"]`. Every entry `nros new` writes has it,
  and nothing depends on `rustapp` as a library.

(`nros-c`, with `["staticlib", "cdylib", "lib"]`, and the
`nros-tests/fixtures/multi_pkg_workspace_*` packages have the same shape and
are not measured here.)

`cargo build -v` shows it: nros-cpp as-is gets `--crate-type staticlib
--crate-type lib` and no `-C lto`. `cargo rustc --crate-type staticlib` on the
same crate gets `-C lto=fat`.

The `lib`/`rlib` was kept on purpose for size inspection (the maintainer's
account). This issue asks for both: LTO on shipped images, and size inspection
that still works.

## (a) The win, measured (2026-11-10, Rust 1.99.0)

| image | as-is | LTO applied | delta |
| --- | ---: | ---: | ---: |
| **linked** nucleo-f767zi Zephyr image (`nros new --lang rust --rmw zenoh --template action-server`), `.text` | 375,560 B | 314,788 B | **−60,772 B (−16 %)** |
| same image, `.data` | 4,605 B | 4,596 B | −9 B |
| same image, `.bss` | 278,179 B | 276,932 B | −1,247 B |
| same image, FLASH as the Zephyr link reports it | 380,176 B | 319,392 B | −60,784 B |
| `nros-cpp` archive, thumbv7em release (`alloc,log,panic-platform,rmw-cffi,platform-freertos`), `.text` summed over members | 985,938 B | 644,812 B | −341,126 B (−35 %) |

The archive figure overstates the win, because the linker's `--gc-sections`
drops unreferenced code anyway. The linked image is the real number. Both LTO
builds were only built, never run. For the nucleo image, the crate-type was
changed in a scratch project's generated `zephyr_entry/Cargo.toml` and the
image rebuilt with `ninja` alone.

## (b) What size inspection loses under LTO

`scripts/nros-mem-report.py --json`, run on the two nucleo images above:

| | as-is | LTO | delta |
| --- | ---: | ---: | ---: |
| RAM attributed to symbols | 282,724 B | 281,468 B | −1,256 B |
| RAM symbols | 404 | 293 | −111 |
| `.text` symbol total | 316,698 B | 268,996 B | −47,702 B |

- **RAM attribution survives.** Every per-owner (crate) RAM total either stays
  the same or falls, and no bytes move into the `(C / asm / no path)` bucket.
  The 117 symbols that vanish (1,278 B) are really removed: mostly
  per-call-site `__NROS_LOG_THROTTLE` statics (32 to 64 B each) and
  `nros_log::ADDED`. Six small statics appear (22 B). Symbol names survive
  LTO, and RAM is what mem-report attributes.
- **What is lost is per-crate `.text`.** After LTO a function inlined into
  another crate's function counts toward that crate, and the merged module is
  one object, not one per crate. mem-report does not attribute `.text` per
  crate today (it reports `text_symbol_total`), so nothing in it breaks.
  `cargo bloat`-style per-crate code size would blur.
- The `nros-sizes-build` probe already works under fat LTO (the root
  profile's own comment, phase 234 / issue 0023).

## (c) Options

1. **Build the image's archive with `cargo rustc --crate-type staticlib`, and
   leave the manifests alone (recommended).** cargo applies the profile's LTO
   when that invocation asks for only a staticlib.
   - The `lib`/`rlib` stays for dependents, `cargo check` and inspection.
   - The seams already exist. Corrosion's `corrosion_import_crate(... CRATE_TYPES
     staticlib)` uses `cargo rustc --crate-type=staticlib`. The Zephyr road's
     `rust_cargo_application` is our patched module, whose `CARGO_ARGS build`
     becomes `rustc --crate-type staticlib`.
   - Cost: one argument per road. Each road must carry it (canonical build
     path: three roads, three carriers), so a gate should assert each road
     passes it.
2. **Drop the `rlib` from the entry template, and add an inspection
   profile.** The rlib is not what keeps per-crate objects: `lto = "off"` is.
   A staticlib built with LTO off still holds one object per crate. So the
   entry template can be `["staticlib"]` (nothing depends on `rustapp`), and a
   `nros-inspect` profile (`inherits = "release"`, `lto = "off"`) gives the
   per-crate view on demand.
   - Does not work for nros-cpp, whose `lib` has dependents.
3. **A separate staticlib-only crate for the image** (a thin
   `nros-cpp-image` re-exporting nros-cpp).
   - Works, but it is one more crate per language surface, plus its
     publishing and lockfile upkeep.
   - Option 1 gets the same result with an argument.

Recommendation: option 1 on every road, plus option 2's `nros-inspect`
profile for whoever wants per-crate `.text`. Acceptance for whichever is
chosen: a linked image that RUNS, the `.text` delta measured on it, and
`mem-report` per-owner RAM unchanged except for removed symbols.

## (d) What the stack-floor tool (issue 2400) needs under LTO

Under fat LTO, `-C save-temps` keeps the merged module in three stages:

- `<crate>-<hash>.<crate>.<cgu>.rcgu.lto.input.bc`: merged, before
  optimisation
- `….rcgu.lto.after-restriction.bc`
- `<crate>-<hash>.<crate>.<cgu>.rcgu.bc`: the post-LTO, post-optimisation
  module that went to codegen (1.7 MB here)

The tool takes only that last module when an `.lto.input.bc` is present. The
other crates' modules beside it never reached codegen.

Measured on the nucleo action-server image with LTO applied, OPEN walked
under the table's policy:

| | OPEN |
| --- | ---: |
| `-Z emit-stack-sizes` (the shipped archive's own frames) | 22,560 B |
| stable: llc over the post-LTO `rcgu.bc` | 22,564 B |
| without LTO (today), for comparison | 26,232 B |

So the tool works under LTO, within 4 B and on the safe side. LTO also
lowers the static OPEN path by about 3.7 KB. If LTO is enabled, every entry
in `config/stack-floors.toml` has to be re-walked. The table's STALE check
reports exactly that.

Nothing here changes a crate type. That waits on the choice above.
