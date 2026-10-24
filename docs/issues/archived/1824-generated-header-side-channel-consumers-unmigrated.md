---
id: 1824
title: "The per-build config headers still go through the `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/` side channel on every lane except Zephyr"
status: resolved
type: tech-debt
area: [build, cmake]
severity: medium
found: 2026-10-12
resolved: 2026-10-24
related: [phase-424, 0945, 0491, 0805, 0360, 0834, 0978, 0985, 0987, 1031, phase-449]
---

**Home: [phase-449](../../roadmap/phase-449-state-keyed-by-name-not-by-tree.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What is left

[phase-424](../../roadmap/archived/phase-424-build-graph-freshness-truth.md) was
archived on 2026-10-11 with three "carried forward" items. Item 2 was superseded
by phase-429 / RFC-0090, and item 3 was closed on 2026-09-06. **Item 1 is still
open, and this issue carries it.** Item 1 is #0945 item 4: the generated
headers' consumer migration.

`nros-{c,cpp}`'s build scripts write their per-build config headers into
`$CARGO_TARGET_DIR/nros-{c,cpp}-generated/` through
`write_header_to_target_dir` (`packages/tooling/nros-build-helpers/src/shared.rs`).
That path is inside cargo's tree, but cargo does not manage it. `$OUT_DIR` is
cargo's own answer, and the Zephyr lane already uses it
(`cargo-out-dir-headers.py`). The other lanes do not.

**Size, re-measured 2026-10-12:** `git grep -n 'nros-c-generated\|nros-cpp-generated' -- ':!docs'`
gives **240 hits in 43 files**. phase-424 counted 128 hits in 26 files on
2026-09-05, so the channel has grown since it was carried forward.

## Why it is a design question, not a sweep

`write_header_to_corrosion()` already writes to `$CORROSION_BUILD_DIR`, which
cmake owns and which is outside cargo's tree. That is not enough on its own. The
target-dir copy is load-bearing because `CORROSION_BUILD_DIR` cannot be watched:
it is a path variable, and watching one as text is issue 0491's defect (issue
0805 measured ~70 spellings in one fingerprint namespace). So the Corrosion
lanes need a header destination that is safe on a cache hit, the way the Zephyr
lane got one, and Corrosion invokes cargo itself rather than being wrapped.

The risk is the one phase-424 named. The side channel has no OWNER, and that
has bitten six times (issues 0360, 0834, 0978, 0985, 0987, 1031). The
differential freshness probes cannot see it: a wrong header comes out as the
same wrong bytes twice, so the verdict is FRESH.

## What closes it

Every lane reads its config headers from a destination that cargo or cmake
owns, and the target-dir copy is deleted. The `git grep` above then returns
nothing outside the helper being retired.

## Re-measured 2026-10-23: the count is not the finish line

The 240 to 93 drop since 2026-10-12 is NOT migration. Issue 1852 (2026-10-12)
made the directory NAMES single-sourced (`nros_build_paths::artifact`,
`cmake/NanoRosArtifactLayout.cmake`, `scripts/lib/artifact_layout.py`), so the
readers stopped spelling `nros-c-generated` and the grep stopped finding them.
The headers still go where they went: `write_header_to_target_dir`
(`packages/tooling/nros-build-helpers/src/shared.rs`) still writes the
`$CARGO_TARGET_DIR` copy and its `.h.stamp`. The grep in "What closes it" can
reach zero with nothing migrated, so the close criterion is restated below.

## Who still reads the side channel (2026-10-23)

| road | reader | from |
| --- | --- | --- |
| cargo-native (NuttX FFI, any `links` consumer) | `DEP_NROS_{C,CPP}_CONFIG_INCLUDE` | **`$OUT_DIR`** already (issue 1569) |
| Zephyr west | `cargo-out-dir-headers.py` placer | **`$OUT_DIR`** already (phase-400 W5.c) |
| cmake / Corrosion | `nros_config_header_mirror` in `packages/api/nros-{c,cpp}/CMakeLists.txt`; `_nros_generated_header_dir` in `NanoRosNodeRegister.cmake` / `NanoRosVerbs.cmake` | side channel |
| NuttX app build | `integrations/nuttx/Make.defs`: `nros artifact c-include --target-dir …` into `CFLAGS` | side channel |
| PX4 module | `integrations/px4/NanoRosPx4Module.cmake`, after a cargo build the USER runs | side channel |

So three roads are left, not "every lane except Zephyr".

## The premise the design rests on, measured

The Zephyr lane works because cargo reports `out_dir` on its JSON stream even on
a fully cached run. Corrosion does not run `cargo build`, it runs `cargo rustc`,
so that had to be re-measured for the verb Corrosion uses. Measured on main
(`cargo rustc -p nros-cpp --lib --crate-type staticlib --target
x86_64-unknown-linux-gnu --no-default-features --features
alloc,env,log,panic-platform,rmw-cffi,platform-posix
--message-format=json-render-diagnostics`, scratch target dir):

| run | wall | rebuilt units | `build-script-executed` for nros-c / nros-cpp | header in `out_dir` |
| --- | --- | --- | --- | --- |
| cold | 80.6 s | 128 | 2 | `nros-c-generated/rclc/detail/nros_config_generated.h`, `nros-cpp-generated/rclcpp/detail/nros_cpp_config_generated.h` |
| warm | 0.65 s | **0** | **2** | same paths, same dirs |

So a cache hit, the case that breaks the side channel (issue 0805's shared
target dir), still names the directory holding the right header.

## Proposal

**D1. Corrosion: wrap ONLY the two crates that write headers, through
Corrosion's own per-target seam.** Corrosion reads
`INTERFACE_CORROSION_CARGO` per imported target (`cargo_bin` in
`Corrosion.cmake`: `$<IF:$<BOOL:${cargo_override}>,…>`), and passes no
`--message-format` of its own (it passes `--verbose` only). Set it on `nros_c` /
`nros_cpp` to `scripts/build/cargo-out-dir-headers.py --package <crate> --dest
<cmake-owned dir> -- <real cargo>`. That is the placer the Zephyr lane already
uses, so there is one reader of the stream (`scripts/lib/cargo_out_dir.py`), not
a second. The mirror's SOURCE moves from the cargo target dir to `--dest`, a
directory cmake names at configure and owns. The real cargo comes from
`_CORROSION_CARGO`, never PATH. Not `Rust_CARGO`: that would wrap every crate in
the configure, including user crates that write no header.

The override reaches only the build: Corrosion's `cargo metadata` at configure
runs `"${_CORROSION_CARGO}" metadata` directly (`CorrosionGenerator.cmake:31`),
and the target property does not exist yet when it runs. Open, to settle on the
first measured build: the mirror's always-run node (issue 1783) stays, because
its edge is still a file no cmake rule produces; only the file's location
changes, and that has to be shown with a header-only change rebuilding exactly
its consumers.

**D2. NuttX: wrap the invocation the Makefile already owns.**
`integrations/nuttx/Makefile` mirrors upstream `RUST_CARGO_BUILD` with its own
command (line ~188), so the placer goes in front of it with `--dest` under the
image build dir, and `nros artifact c-include` answers that dir. One spelling,
the same script.

**D3. PX4: the module cannot wrap a cargo it does not run.** The user runs the
documented `cargo build -p nros-cpp …` (the module restates it at lines ~109 /
~219 as the remedy). Two options:
- (a) the documented command becomes the placer form (`cargo-out-dir-headers.py
  … -- cargo build …`), so the user's own build places the header;
- (b) the module re-asks cargo at configure with the same arguments: a cached
  `--message-format=json` run is 0.65 s, measured above.

(a) keeps one producer; (b) keeps the user's command unchanged but spells the
argument list a second time, inside the module. Recommend (a); PX4 is the road
with the fewest users and a single remedy string to change.

**D4. Delete the side channel last, in one change.** Delete
`write_header_to_target_dir`'s target-dir half, its `.h.stamp`
(`write_stamp` / `drop_stamp_without_header` / `write_header_if_absent_or_verify`'s
cross-writer check, which exists only because two writers share that path), and
retarget or retire the gates that police it: `check-orphan-generated-stamp`,
`check-px4-archive-header-pairing`, `check-sizes-header-mirrors`. Issue 0834's
absorbing stamp-without-header state, one of CLAUDE.md's two documented
`rm -rf` exemptions, disappears with the stamp.

**Order:** D1 (covers every cmake leaf, the largest consumer set), then D2, then
D3, then D4. Each before D4 is additive: the side channel keeps working until
nothing reads it.

## What closes it (restated)

`write_header_to_target_dir` writes nothing under `$CARGO_TARGET_DIR`, and no
reader resolves a generated header through `--target-dir`. Measure it with the
reader table above, not with a grep for the directory name: issue 1852 already
made that grep miss every reader that asks for the name.

## D1 landed (2026-10-24): the cmake / Corrosion road reads `$OUT_DIR`

`nros_config_header_place(<target> <dest> <package>...)`
(`cmake/NanoRosConfigHeaderMirror.cmake`) sets `INTERFACE_CORROSION_CARGO` on
`nros_c` and `nros_cpp` to a generated wrapper that runs Corrosion's own
`_CORROSION_CARGO` under `scripts/build/cargo-out-dir-headers.py`, and adds
`--message-format=json-render-diagnostics` through `corrosion_set_cargo_flags`,
which Corrosion places before its rustc `--`. The mirror's source is the placed
copy, passed with `-` as the gen-subdir, which `mirror-generated-header.sh`
reads as "the only source": no `cargo/*` candidate, and a missing one is an
error naming this issue.

Two things the design did not say, both measured:

- **The C++ image needs TWO packages placed.** nros-cpp's build script writes
  only the C++ header; the C one the C++ image also mirrors comes from nros-c's
  build script, a dependency in the same cargo run, into nros-c's own
  `$OUT_DIR`. The placer takes `--package` repeatedly now.
- **`check-sizes-header-mirrors` would have hidden drift in a placed mirror.**
  Its "shared copy gone" exemption fires on `src == leaf`, which is always true
  for a placed source. Restricted to the old fallback; a self-test case covers
  it, and reverting the restriction fails that case.

Measured on `examples/linux/{c,cpp}/talker` (zenoh, Ninja): both build and link;
the placed and mirrored headers are byte-identical; a no-op rebuild is 0.42 s
with no compile; with the placed copies deleted, a rebuild on a fully cached
cargo (0.07 s) places all three again. Appending a line to the header in
cargo's `$OUT_DIR` recompiles `main.cpp.o` and relinks, nothing else; removing
it does the same. The always-run mirror node (issue 1783) stays.

Left: D2 (NuttX), D3 (PX4), D4 (delete the side channel and its stamp). The
cargo-dir half of `mirror-generated-header.sh` has no Corrosion caller now and
goes with D4.

## D2 landed (2026-10-24)

`integrations/nuttx/Makefile` runs its cargo build under
`scripts/build/cargo-out-dir-headers.py` with `--dest
<image root>/placed-<flavor>`, and `Make.defs` asks `nros locate artifact
{c,cpp}-include --placed-dir <that dir>` — a new flag that resolves the header
artifacts under a placed directory and refuses an archive (an archive is
cargo's output and is never placed). The layout under the placed dir is the
same `nros_build_paths::artifact` join, so no path is spelled twice.

The build did not run on main before this, for two reasons unrelated to the
headers, both found by running it (no lane runs this file —
`check-nuttx-integration-makefile.sh` says so):

- it asked the image crate for `std`, which phase-359 W10 stage C deleted from
  nros-c / nros-cpp the day before. Now `alloc;panic-platform`, the FFI crate's
  set.
- with `CONFIG_NROS_CPP_API=y` it generated BOTH image crates, each in its own
  cmake process, and each wrote the shared root's `members` from that process
  alone, so the C crate was outside its root and cargo refused it. Only the C++
  archive links (issue 1636), so only the linked flavor is built now.

Measured on a private copy of the NuttX tree, `qemu-armv7a:nsh`, zenoh: C and
C++ configurations both build and link; the placed headers are byte-identical
to the side channel's; the compile line of the app's C TU carries the placed
`-I` and no target-dir one; with the placed dir deleted, a re-run of `context`
on a fully cached cargo places both headers again (6.3 s). NuttX runs
`context::` once per tree (`$(APPDIR)/.context`), so an incremental `make` runs
no cargo at all — that was already true on main, for the archive as well as
the headers.

Also found, for D4: `just/check/lanes.just` (`check-c` / `check-cpp`, and the
param-dir probe near line 967) reads `nros_locate_artifact c-include` /
`cpp-include` without a placed dir, i.e. the side channel. The reader table
above missed it.

Left: D3 (PX4), D4.

## D3 landed (2026-10-24)

Option (a). `NanoRosPx4Module.cmake` locates the four header artifacts with
`nros_locate_artifact(... PLACED_DIR <dir>)` (the cmake spelling of
`--placed-dir`), where `<dir>` is `NROS_PX4_HEADER_DIR` (cache, then env, like
`NROS_CPP_ARCHIVE`), default `build/nros-px4/placed`. The archive command the
module documents and names in every remedy is now ONE string,
`_NROS_PX4_UORB_ARCHIVE_CMD` — the placer in front of the same `cargo build -p
nros-cpp …`. It was restated at three sites before. `just px4
build-sitl-example` runs that form; `build-bridge-example` places into
`<image>/placed` and exports `NROS_PX4_HEADER_DIR`, as it already exported
`CARGO_TARGET_DIR` for the archive. That recipe also asked the image crate for
`std` (gone since phase-359 W10 stage C), so it stopped in cargo; it now asks
for the uORB set plus `bridge`.

The configure-time pairing assertion (issue 1046) stays, and is what catches the
hole left by construction: a plain `cargo build` moves the archive and not the
placed headers.

Measured without a PX4 tree, through a stub project that includes the module
(its file-scope half — locate, archive, pairing — is everything this changes;
`nros_px4_add_module` needs `px4_add_module`):

| state | pre-D3 module | D3 module |
| --- | --- | --- |
| uORB archive built by the placer form | — | both headers pair |
| then a DEV build with other features in the same target dir (its build script ran, its compile failed) | **FATAL**: pairs the release archive against the dev build's header | both headers still pair |
| then a plain release build with other features | — | **FATAL**, naming the placer command |
| that command, run verbatim | — | both pair again (7.2 s, cached) |
| bridge image (zenoh, jazzy) through the recipe's step 3 | — | both pair, archive from `CARGO_TARGET_DIR`, headers from `NROS_PX4_HEADER_DIR` |

Not run: a PX4 SITL build (`make px4_sitl_default`). The only PX4 tree here is
the main checkout's shared one, and reconfiguring it for another worktree's
`EXTERNAL_MODULES_LOCATION` would rebuild somebody else's configured tree.

Left: D4. Census of what still reads `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/`
(2026-10-24, `git grep` over non-doc files), each to migrate to a placed dir or
to the `links` channel before the writer goes:

- `just/check/lanes.just` — `check-c` / `check-cpp` (`nros_locate_artifact
  {c,cpp}-include`, no placed dir) and the param probe (`CARGO_TARGET_DIR=…
  nros_locate_artifact c-include`);
- `scripts/build/compile-check-fixtures.sh` — `{c,cpp}-config-header` twice;
- `packages/testing/nros-tests/fixtures/borrowed-e2e/build.sh` —
  `c-config-header`;
- `packages/cli/rosidl-codegen/tests/heap_compile_check.rs` — a literal
  `target/nros-c-generated`, three times (and outside the artifact helper);
- `zephyr/CMakeLists.txt` / `cmake/NanoRosNodeRegister.cmake` through
  `_nros_generated_header_dir()` — to check: phase-400 W5 points the Zephyr
  answer at the placer's root, so these may already be off the side channel.

The writer side and its policing are D4 as designed: `write_header_to_target_dir`'s
target-dir half, the `.h.stamp`, `check-orphan-generated-stamp`,
`check-sizes-header-mirrors`' target-dir candidates, the cargo-dir half of
`mirror-generated-header.sh`, and the staleness probe's `ConfigHeaderStamp`
exemption.

## D4 landed (2026-10-24): the side channel is deleted — RESOLVED

Every reader moved first, then the writer went.

**The reader the census missed was a WRITER's own check.** nros-cpp's build
script verified its c-format header against the target-dir copy nros-c had
left (`write_header_if_absent_or_verify`), and the `.h.stamp` existed only to
tell that copy's staleness from real divergence. It now reads nros-c's copy from
nros-c's OWN `$OUT_DIR` through the `links` channel
(`DEP_NROS_C_CONFIG_INCLUDE`), which belongs to the same cargo invocation, so a
disagreeing define has one meaning and stops the build, and the stamp has
nothing left to answer (`verify_against_nros_c_header`; unit-tested, and the
test fails when the comparison is disabled). nros-cpp no longer writes the C
header anywhere.

Migrated, each to a placed dir from its own cargo run (one shell spelling,
`nros_cargo_placed` in `scripts/build/cargo.sh`): `check-c` (both builds and the
param probe), `check-cpp`, `compile-check-fixtures.sh`'s cxx-syntax generation,
`borrowed-e2e/build.sh`, `size_probe_verify.sh`. `heap_compile_check.rs` (a
manual `#[ignore]` test in the CLI workspace, which runs no build of its own)
takes `NROS_C_CONFIG_INCLUDE` and refuses, naming the two commands, when it is
unset; its `check-artifact-path-literals` allowlist row went with the literal.

**One more found while deleting: a C++-only Zephyr image.** Its C header was
declared a byproduct of the nros-cpp cargo build, but the placer there named
only `--package nros-cpp`, so the file arrived through the target-dir copy —
which on Zephyr IS the header root. The nros-cpp build now places `nros-c` too,
exactly when no separate `nros_c_cargo_build` exists (the same condition the
byproduct already used).

Deleted: `write_header_to_target_dir`'s target-dir half and
`write_shared_header`, `target_dir_path`, `drop_stamp_without_header`, the stamp
helpers and `probe_artifact_stamp` (`nros-build-helpers`); the gate
`check-orphan-generated-stamp` (retired from the registry baseline: it policed
a file nothing writes); the staleness probe's `ConfigHeaderStamp` exemption;
`check-feature-sweep`'s per-run rmtree of the shared dirs; the cargo-dir half of
`mirror-generated-header.sh` (a gen-subdir other than `-` is now refused, naming
this issue) with `check-sizes-header-mirrors`' matching candidates and
`nros_config_header_cargo_manifest`, which existed only to feed it.

**The regression gate is the CLI.** `nros locate artifact` REFUSES a header
artifact without `--placed-dir` (there is no target-dir copy for it to name),
so a reader added later fails at its first run, naming the placer.

Issue 0834's absorbing stamp-without-header state is gone with the stamp, and
CLAUDE.md's `rm -rf` rule is down to one exemption.

Measured: `just check c` and `just check cpp` pass and read only their placed
dirs, and no `nros-{c,cpp}-generated/` exists under any target dir after them;
`examples/linux/cpp/talker` (zenoh, Ninja) builds and links with both crates'
headers placed, a no-op rebuild is 0.61 s and compiles nothing, and the build
tree holds no side-channel dir and no `.h.stamp`; the three heap C syntax checks
pass through the documented remedy; every touched script's self-test passes.
