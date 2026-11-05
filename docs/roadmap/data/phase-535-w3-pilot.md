# Phase-535 W3 pilot: `mps2-an385-freertos`, captured 2026-11-05

The pilot asks whether moving one platform's example pairs into the language
workspaces' behaviour bringups (RFC-0122 D1) makes that platform's fixture
build cheaper (RFC-0122 D6). The phase doc sets the bar: a saving under 10 %
stops the phase for a maintainer decision before W4.

**Result: mixed, and under the bar on the number the lane pays.** Wall clock
got worse in every run, by 19 % to 102 % (+63 % on the means). Build CPU fell
by 14 % on the means, and by 20 % in the two runs the host's load left alone. The CPU saving is
real but costs wall time: the builder serialises a workspace's rows (see
below). By the phase doc's rule, the phase stops here for a maintainer
decision.

**Re-measured with the builder fanned out (option 1, the maintainer's
choice): still under the bar.** The fan-out recovered most of the wall clock
the pilot lost: 846 s against 1258 s with the serial builder. But it is still
3.8 % slower than the leaves on the same builder, while CPU stays 17 % lower.
See "Re-measure with the fan-out builder" below.

## Method

`scripts/dev/measure-fixture-build.sh --platform freertos --no-sccache`, which
this pilot adds:

- It wipes the artifact root of every `freertos` row in both row tables
  (`fixtures-manifest.py coords`).
- It also wipes the platform's shared cargo group (`build/cargo-fixtures/freertos`)
  and its Corrosion cargo target (`build/corrosion-cargo/freertos`). The first two
  attempts missed these, which left the Rust leaves and the CMake roads' Rust
  half warm, so those numbers are discarded.
- It times `just freertos build-fixtures` with the rustc wrapper cleared. It
  records wall clock and the build's own user+sys CPU (`/usr/bin/time`), because
  this host is shared and its load moved between 7 and 220 during the session.
- The CLI and the launch resolver are rebuilt first, outside the timed section,
  as in RFC-0066's method.

The two trees:

- **Before:** `origin/main` at `cfa653422d`. 18 pair leaves under
  `examples/mps2-an385-freertos/{rust,c,cpp}/` plus every other `freertos` row
  (40 rows).
- **After:** the pilot branch (draft PR #3129). The 15 non-talker leaf rows are
  removed, and the
  three talkers stay as witnesses. 18 image rows are added in
  `examples/workspaces/{rust,c,cpp}/src/{pubsub,service,action}_bringup`, 43 rows
  in all.

Host: Intel Core Ultra 7 265K, 20 threads.

## Numbers

Every run is fully cold: 31 trees wiped before, 27 after. Runs were alternated
on one host. Another agent shared the host, which is the reason for the CPU
column and the load column.

| run | tree | wall | CPU (user+sys) | load at start |
| --- | --- | --- | --- | --- |
| r3 | before | 735 s | 4349 s | 8.2 |
| r4 | before | 807 s | 4264 s | 10.1 |
| r1 | after | 1557 s | 4297 s | 14.6 (rose past 100 mid-run) |
| r2 | after | 1296 s | 3395 s | 11.4 |
| r4 | after | 921 s | 3464 s | 12.8 |

| | before (mean of 2) | after (mean of 3) | change of the means | after, r2 and r4 only |
| --- | --- | --- | --- | --- |
| wall | 771 s | 1258 s | **+63 %** | 1109 s (+44 %) |
| CPU | 4306 s | 3719 s | **-14 %** | 3430 s (-20 %) |

Every run exited 0. The two discarded attempts, before the wipe covered the
shared Rust state, measured 1764 s and 578 s wall (3786 s and 2558 s CPU).
They are kept only so nobody repeats them.

## Where the CPU went, and why the wall clock is worse

1. **The C/C++ images share a configure, and that is the CPU saving.** One
   configure per language builds the FreeRTOS kernel, lwIP and the C glue once
   for six images. The second image of a configure is a link of about 8 s,
   against 22 to 29 s for the first. The Rust half of the CMake roads was
   already shared before the move: every FreeRTOS configure, leaf or workspace,
   builds `nros-c`, the RMW and zenoh-pico's Rust side into one Corrosion target
   (`build/corrosion-cargo/freertos`). That bounds what grouping can save.
2. **Rust should save nothing.** It was not measured on its own. The six Rust pair leaves already built into one
   shared cargo group (`build/cargo-fixtures/freertos`). A Rust workspace image
   is its own cargo root with its own target dir (issue 0616, RFC-0122 D6), so
   the six images build their dependency graph six times where the leaves built
   it about once.
3. **Wall clock is worse because the builder serialises by workspace
   directory.** `workspace-fixtures-build.sh` runs every row of one workspace
   directory in one group worker, serially, because the rows share a sync, a
   cargo config and a CMake tree. The 18 leaves ran in parallel make leaves. As
   images they join three serial chains, which grow from 3, 5 and 1 rows to 9
   (`rust`), 11 (`cpp`) and 7 (`c`).

## What else the pilot found

- **The CMake road had no per-image locator or network.** The generated root
  emitted `DOMAIN_ID` per entry (issue 1946), but the locator and the address
  were configure-wide (`-DNROS_ENTRY_LOCATOR`, `-DNROS_IMAGE_NET`). Two images
  of one configure could not dial different routers or hold different
  addresses. #3129 emits `LOCATOR` and `IMAGE_NET` per entry, and the image's
  locator outranks the `-D` in `_nros_resolve_entry_locator`. Any multi-image
  configure on an RTOS needs this, whatever is decided about the move.
- **A moved leaf has to be deleted with its row.** `check-example-build-coverage`
  (issue 1650) refuses a leaf that no row builds, so the leaf deletions planned
  for W7 happen per platform, in the PR that moves its rows.
- **The pilot works.** On the pilot fixtures, all 10 FreeRTOS cells of
  `rtos_e2e` passed with unchanged case names. The one change was the C++
  listener's readiness marker (`output::LISTENER_READY_MARKER`).

## What would change the answer

None of these was tried; each is a design change, not a pilot.

- **Group the builder by configure, not by workspace directory.** Rows on
  different configures or Rust roots of one workspace could run in parallel
  once the per-workspace `nros configure` has run. This should recover the
  wall clock and keep the CPU saving. It is the change most likely to pass the
  bar.
- **Share a Rust target across images of one (board, rmw).** RFC-0122 D6 allows
  this only behind a gate proving that each entry resolves alone to the
  feature set it resolves with the others. It would recover the Rust half.
- **Leave the pairs where they are.** On CMake, `corrosion-cargo` had already
  taken much of the saving RFC-0122 D6 hoped for. On Rust, the shared cargo
  groups had taken all of it.

## Re-measure with the fan-out builder

Captured 2026-11-05, after the maintainer chose option 1.

### What changed in the builder

`workspace-fixtures-build.sh` now fans out by **build unit**, not by workspace
directory (`scripts/build/workspace-fixture-units.py`). Rows that write a common
cmake configure, cargo target dir or generated entry are one unit and stay
serial. Units run at once under the same make fifo jobserver.
`just freertos build-examples` makes one builder call for every `freertos` row
instead of one per language, so the languages overlap too. On the pilot tree
the 32 `freertos` workspace rows form 19 units. The longest are the C and C++
`freertos-zenoh-mps2-an385` configures, 7 rows each.

The fan-out found a hang on the way: issue 2284. An sccache server started
inside a make jobserver keeps its tokens, and the make never exits. The
CycloneDDS units hit it on their first run under a pool. It is fixed in the
same pull request, and it is very likely issue 1403's cause.

### Method

Same script and flags as above (`measure-fixture-build.sh --platform freertos
--no-sccache`; the copy from #3129 on the BEFORE tree, whose own copy lacks
`--platform`). Every run cold (24 to 31 trees wiped). Runs alternated on one
host. Load was sampled every 30 s through each run, besides `uptime` before and
after.

The two trees, chosen on purpose to differ ONLY in the layout:

- **Before:** `main` + the fan-out builder + the issue 2284 fix (this
  pull request's code).
  The 18 pair leaves, 40 rows.
- **After:** #3129 rebased onto that, plus one fix: its
  `build-examples` still ran `nros configure` over five retired Rust leaf
  directories. This passed only on a checkout that still had them on disk. 43
  rows.

`--no-sccache` clears the rustc wrapper only. `ProvideCycloneDDS.cmake` still
caches CycloneDDS's C/C++ through sccache, on both trees alike.

### Numbers

| run | tree | wall | CPU (user+sys) | 1-min load: start / peak / mean | kept |
| --- | --- | --- | --- | --- | --- |
| f2-1 | before | 2239 s | 4407 s | 29.2 / ~52 / — | **no** |
| f2-2 | after | 1123 s | 3744 s | 20.9 / 43.7 / 25.5 | yes |
| f2-3 | before | 896 s | 4465 s | 24.3 / 41.2 / 27.1 | yes |
| f2-4 | after | 719 s | 3699 s | 22.9 / 32.3 / 20.4 | yes |
| f3-1 | before | 735 s | 4537 s | 27.9 / 46.4 / 25.1 | yes |
| f3-2 | after | 697 s | 3770 s | 15.3 / 45.0 / 24.1 | yes |

Every run exited 0.

f2-1 is discarded. Its 1-min load reached 41 to 52 mid-run, more than 2 x
nproc (20 threads). For minutes, another session's `cargo test` held the
user-wide cargo package-cache lock while blocked in a btrfs log commit (state
D). Every cargo in this build queued behind it, and units finished in
lock-step batches (362/362, 690/690, 816/816/816 s).

The other peaks reach 41 to 46 for single 30 s samples. That is about 2 x
nproc, and the build's own pool contributes to it. Their means stay at 20 to
27, so they are kept. A strict cut at 40 would keep only f2-4, which leaves
no comparison at all.

| | before (f2-3, f3-1) | after (f2-2, f2-4, f3-2) | change of the means |
| --- | --- | --- | --- |
| wall | 816 s | 846 s | **+3.8 %** |
| CPU | 4501 s | 3738 s | **-17 %** |

The interleaved pairs: f2-3/f2-4 is -20 % wall, and f3-1/f3-2 is -5 %. The
first after run of the series (f2-2) followed the discarded run and is the
slowest. Without it, after is 708 s, -13 %. Dropping it would be choosing runs
for their result, so the decision uses the means above.

Against the serial-builder numbers above, the fan-out cut the pilot's wall
clock from 1258 s to 846 s (-33 %). The leaf tree gained nothing (771 s to
816 s, within noise): its rows were already parallel leaves.

### Where the wall clock still goes

From the row times in f3-1 and f3-2:

- The workspace stage got longer: about 150 s before, about 350 s after. The
  leaf stage got shorter by about as much, because it has fewer leaves.
- **The critical path is one configure's rows, which are serial by design.**
  In the C and C++ `mps2-an385` configures the first image takes about 205 s.
  The other six follow one after another: 52 + 10 + 24 + 10 + 24 + 8 s, about
  130 s. Each is its own `nros build` and `cmake --build`.
- **The CMake units do not overlap as much as the unit count suggests.** In the
  BEFORE runs, the 9 cmake units finished within 30 s of each other (f3-1: 117 to 145 s). Every
  FreeRTOS configure builds its Rust half into the one Corrosion target
  (`build/corrosion-cargo/freertos`), and cargo locks a target dir.
- The leaf stage and the workspace stage still run one after the other in
  `build-examples`.

What would move it further. None of these was tried; each needs a decision:

1. Build all the images of one configure in one `cmake --build` (one ninja
   graph, all entries), not one `nros build` per row.
2. Start the workspace builder beside the leaf builder in `build-examples`.
3. Share a Rust target across the images of one (board, rmw), as in the
   section above.

## Raw logs

Not committed (`tmp/` is gitignored). They are the `tmp/p535/w3-*.md` and
`tmp/p535/w3-*-build.log` files in the measuring checkout. The re-measure is `tmp/p535/fo-f2-*`, `tmp/p535/fo-f3-*`
(`.md`, `.meta` with `uptime`, `-build.log`) and `tmp/p535/load-f{2,3}.log`.
