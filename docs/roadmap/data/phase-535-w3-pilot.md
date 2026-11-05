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

## Raw logs

Not committed (`tmp/` is gitignored). They are the `tmp/p535/w3-*.md` and
`tmp/p535/w3-*-build.log` files in the measuring checkout.
