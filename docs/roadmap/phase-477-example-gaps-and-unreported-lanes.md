# Phase 477 — example gaps and unreported lanes

**Status (2026-10-03). All work items open.** Successor to
[phase-470](archived/phase-470-example-layout-unification.md), which landed the
example taxonomy and generated every Zephyr workspace entry it could build
(W1–W6). This phase holds what that work FOUND but did not own, regrouped by
theme rather than by the order it was discovered. Implements no new RFC.

**Source:** a build-path review on 2026-10-03 — a census of all 192 example
roots on `main`, each classified by its on-disk shape AND by the build road the
fixture manifest declares for it (method in W2) — plus every open issue that
phase-470 filed or touched.

## What the review measured

**The road is the primary classifier.** `canonical-build-path.md` has three
roads (cargo, cmake, west); phase-470 has a layout taxonomy. Joined, the layout
classes reduce to the road plus one sub-axis:

| class | road | members |
| --- | --- | --- |
| 3 — leaf, node-class | cargo | 39 Rust leaves + the 7 cargo-rooted bare-metal C/C++ leaves |
| 3a — leaf, application-shaped | cargo | 16 `native/rust/*` (owns `main`; no `system.toml`) |
| 4 — leaf | cmake | ~95 C/C++ leaves + `rv-virt-threadx/rust/*` |
| 4 + `prj.conf` — leaf | west | `zephyr/{c,cpp,rust}/*` |
| 1 / 1z — workspace image | cargo/cmake / west | 16 workspaces |
| 1b — workspace, no bringup | cmake | 2 templates |
| X — foreign build | PX4's | `px4` |

Two taxonomy errors found and fixed with this phase's opening commit:
**class 3a was missing** (16 consistent leaves, unnamed), and **class 4's member
list was Rust-only** ("every C and C++ leaf" stopped being true when the
bare-metal C/C++ leaves went cargo-rooted). 1z was redefined by ROAD (a Zephyr
image, generated or not) — the old "hand-written Zephyr entry" definition was a
class that existed only because something was unfinished, which the taxonomy's
own "class 2 does not exist" rule forbids.

## Work items

### W1 — entry migration residue

Issue [1288](../issues/1288-zephyr-rust-workspace-entries-not-generated.md):
three hand-written entries remain, none a generator gap.

- **`rust/src/zephyr_entry_robot1`** — add a `cmd::build` unit test that an
  explicit `entry =` outranks `<id>_entry`; then migrate it. It is today the only
  evidence for that rung.
- **`realtime-c/src/zephyr_entry`** — give the `smp_bringup` image its own id
  (the generated dir is keyed on `(platform, rmw)`, so two bringups would collide
  silently) and correct that row's board (declares `native_sim`, builds
  `qemu_cortex_a53`).
- **`realtime-cpp/src/fvp_entry`** — **builds and RUNS on this host** (2026-10-05).
  "Needs a host that can build it" was never true: the model is a pinned public
  download (`nros setup --tool arm-fvp`, 68 MB, x86_64 Linux, no licence, no
  root) and the `aarch64-zephyr-elf` toolchain was already in the Zephyr SDK.
  With it installed, `just zephyr verify-fvp-runtime` →
  `fvp_ws_entry_two_tier_publishes` **PASS**. Getting there surfaced two LIVE
  defects the skipped lane had hidden — `[image.fvp]` inherited `rmw = zenoh`
  while using the Cyclone overlay (refused by the image-agreement check since
  2026-08-30), and a `std::nothrow` double definition against full libstdc++.
  Both fixed (see W3). Remaining: migrate it to a generated west application
  (it needs the `nano_ros_use_board` + `EXTRA_CONF_FILE` axis W5.b3 did not do).

Also in this theme: [1289](../issues/1289-workspace-node-tables-still-in-manifests.md)
(45 node packages still declare their class in `[package.metadata.nros.node]`),
[1520](../issues/1520-ambiguity-example-cited-everywhere-was-the-manufactured-one.md)
(the `entry =` ambiguity docs cite), and
[1511](../issues/1511-no-worked-example-of-entry-customisation.md) (the
customisation tutorial, deferred until the last hand-written entry is gone).

**Acceptance:** 1288 closes — each remaining entry migrated or recorded as a
deliberate exception.

### W2 — examples no lane builds

Issue [1650](../issues/1650-examples-no-lane-builds.md). `workspaces/launch`,
`templates/multi-package-workspace`, `templates/zephyr-byo` and (partly)
`templates/multi-node-workspace` are compiled by nothing. The two TT bridge
examples that were also in this set were DELETED (maintainer decision): their
"fixture" was always a separate `bins/` crate.

Census method, so the next review re-runs rather than re-derives: walk
`git ls-files examples`; a root is `examples/<plat>/<lang>/<name>`,
`examples/workspaces/<ws>` or `examples/templates/<t>`; load the manifest with
`fixtures-manifest.py`'s `load` / `load_workspace_fixtures` /
`load_compile_check_fixtures`; a root is covered when a row's `dir` equals it or
lies under it.

**Acceptance:** each tree has a row or a recorded reason, and a gate keyed on
the census (not a path list) keeps the class closed.

### W3 — lanes that do not report

- Issue [1651](../issues/1651-host-tests-red-or-cancelled-on-most-pushes.md) —
  `host-tests.yml` has not gone green since 2026-06-17: 24 of its last 30 runs
  cancelled by the next merge, the rest failing at `just ci tier1`. The highest-
  severity item in this phase: it is the only lane running the fixture-backed
  `nros-tests` suite on the host.
- Issue [1627](../issues/1627-west-configure-fixture-passes-on-failed-configure.md)
  — a `west-configure` fixture counts a failed configure as built when its
  declared output is written before the generate step.
- **The FVP lanes were exempt on a false premise** (2026-10-05). Five recipes
  (`build-/run-fvp-ws-entry`, `build-/run-fvp-board-import`,
  `verify-fvp-runtime`) were excluded from "a NAMED lane must work" as
  "licence-gated and user-supplied" — false since 2026-09-06, when the model
  became `[tool.arm-fvp]`; the BUILD recipes never needed the model at all. The
  skip hid two live defects (W1). Exemptions removed; the recipes declare
  `nros_lane_platform zephyr`, so a named run without the model now fails naming
  `nros setup --tool arm-fvp`, while a fan-out still skips and reports.
- **A decision 1627 records and nobody has taken:** since PR #1561, a narrowed
  Zephyr lane honours `NROS_ZEPHYR_FIXTURE_FILTER` for the five west compile
  checks, so `just zephyr build-rust-examples` and its siblings no longer build
  them. Keep that (the filter now means what it says) or restore the old
  coverage.

**Acceptance:** `host-tests.yml` reports a `success` on `main` and a regression
as a `failure`; 1627 closed; the filter decision recorded.

**Status 2026-10-05 (issue 1651, W3's host-tests item):** first `test-all`
verdict since 2026-06-17, from a branch dispatch that moves `just check` off
the integration job (run 37252649866): the job completes as a `failure`, not a
timeout-`cancelled`. Reds grouped and filed as issues 1684–1692 (1689 fixed;
the probe self-test fixed). The split itself costs ~+25 runner-min/night, so
it waits on the CI-budget decision; 1651 stays open.

### W4 — invariance gates

- Issue [1509](../issues/1509-node-packages-name-no-platform-or-rmw.md) — the
  node-package gate (phase-470 W7, moved here). A node package names no platform
  or RMW. Now that entries are generated there is one place a platform may be
  named, which is what makes the rule statable.
- Three fixture-free `nros-tests` targets already red on `main` — filed as a
  new issue by PR #1581, which resolves issue 1521 and puts 17 fixture-free
  tests in the merge-queue lane. Cite it by number once #1581 lands.

**Acceptance:** both issues closed.

### W5 — bare metal, the remaining roads

Issue [1512](../issues/1512-c-api-does-not-reach-bare-metal.md) is open for the
**C-rooted** cmake road only: the board has no linker script with `SECTIONS`/
`ENTRY`, no C reset/vector startup, and no C-callable board init (which needs a
new board-seam staticlib, since `nros-c` cannot depend on a board). The six C
roles and a C++ talker exist and boot on the cargo-rooted road (PR #1585); the
other C++ roles are mechanical.

**Acceptance:** 1512 closed, or its C-rooted half recorded as not pursued.

### W6 — board identity residue

Issue [1652](../issues/1652-framework-board-strings-in-names-nuttx-platformio.md)
— NuttX's `qemu-armv7a-nsh` and PlatformIO's `esp32dev` still sit in `names`
the way the Zephyr id did before issue 1519. Needs a typed per-ecosystem field
first.

### W7 — a runtime bug the migration exposed

Issue [1535](../issues/1535-mixed-zephyr-entry-segv-in-cffi-publisher-vtable.md) — the
`mixed` Zephyr image SEGVs on its first publish
(`CffiPublisher::poll_status_events` reads an unusable vtable). Pre-existing: the
hand-written image did the same, and the generated one reproduces it byte for
byte.

## Out of scope, with where they belong

- Issue 1407 (the cmake road writes the sizing descriptor from a poorer
  inventory) — RFC-0100 sizing, not layout.
- Issue 1641 (CLI readers take `$NROS_REPO_DIR` raw and act on the parent
  checkout from a worktree) — the 1510/1280 worktree class.

## Order

W3's `host-tests.yml` item first: until that lane reports, regressions in
everything else here land unseen. W1–W2 and W4–W7 are independent of each other.

## Checkpoint — 2026-10-08 (session stopped here)

**Landed since the phase opened:** #1666 (FVP lanes run; `[image.fvp]` rmw +
conditional `nothrow_tag.cpp`), #1671/#1672 (host-tests `gates` job; two
harness reds), #1699 (lane-first capability skips), #1707 (issue 1700:
shared Corrosion cargo dir keyed on entity facts), #1708 (tier 1's run moves
to `run-matrix.yml`'s self-hosted `tier1` job; host-tests keeps `unit` +
`gates`; supersedes 68c2ff9d0's hosted build, whose first run failed in
`just setup tier1`), #1720 (issue 1703: packed entry handles in five executor
accessors), #1728 (shared fixture groups skip out-of-lane; `lane_scope`
asks the coordinates), #1758 + #1794 (issue 1729: derived-tier `/diagnostics`
reporter count + resolved-seed configure edge; in-lane fixture errors fail
through `RequireFixture::require`; `name-real-failures.py` reads the
failure message only).

**In flight — resume here:**
- **Issue 1746** — branch `fix/1746-shared-cargo-repoint`, one WIP commit,
  NOT verified (draft PR). Next: reconfigure a native cpp leaf and build ONCE
  (must link), `build-test-fixtures` after a reconfigure, `just format`,
  `just ci gate`; then reword the commit and arm. Until it lands, the first
  build after any reconfigure of a native cpp leaf can fail once with
  undefined `nros_config_variant_sz_*` (a second `ninja` settles it).
- **First `run-matrix` tier-1 run** — not yet dispatched (single-occupancy
  self-hosted runner; maintainer's call). It is the CI acceptance for issues
  1684 and 1685; dispatch after 1746 lands.

**Still open in this phase:** W1 (`fvp_entry` to a generated west app; the
`realtime-c` SMP image id + board; robot1 precedence unit test), W2 (1650),
W3 (tier-1 residue 1686–1688, 1690–1692; 1627; the narrowing-filter
decision), W4 (1509, 1644's lane decision), W5 (1512), W6 (1652), W7 (1535).
