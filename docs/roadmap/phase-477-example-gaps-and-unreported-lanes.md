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

**Status 2026-10-10:** `zephyr_entry_robot1` migrated — the precedence test
(`an_explicit_entry_outranks_the_id_entry_package`) landed first, then the
generated entry, whose `.config` differs from the hand-written one only by the
locator the image declares (which the hand-written app had ignored).

Also in this theme: [1289](../issues/1289-workspace-node-tables-still-in-manifests.md)
(45 node packages still declare their class in `[package.metadata.nros.node]`),
[1520](../issues/1520-ambiguity-example-cited-everywhere-was-the-manufactured-one.md)
(the `entry =` ambiguity docs cite), and
[1511](../issues/1511-no-worked-example-of-entry-customisation.md) (the
customisation tutorial, deferred until the last hand-written entry is gone).

**Acceptance:** 1288 closes — each remaining entry migrated or recorded as a
deliberate exception.

### W2 — examples no lane builds

Issue [1650](../issues/archived/1650-examples-no-lane-builds.md). `workspaces/launch`,
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

**Status 2026-10-10:** done — issue 1650 resolved: rows for `launch`,
`multi-node-workspace` and `multi-package-workspace` (which also gained its
missing `.colcon_workspace`), a recorded reason for `zephyr-byo` and five more
roots the census found, and `just check example-build-coverage` (188/194 built,
6 reasoned).

### W3 — lanes that do not report

- Issue [1651](../issues/1651-host-tests-red-or-cancelled-on-most-pushes.md) —
  `host-tests.yml` has not gone green since 2026-06-17: 24 of its last 30 runs
  cancelled by the next merge, the rest failing at `just ci tier1`. The highest-
  severity item in this phase: it is the only lane running the fixture-backed
  `nros-tests` suite on the host.
- Issue [1627](../issues/archived/1627-west-configure-fixture-passes-on-failed-configure.md)
  — a `west-configure` fixture counts a failed configure as built when its
  declared output is written before the generate step. **Resolved 2026-10-10
  (D1):** the rows declare `build.ninja`, gated by `validate-compile-checks`;
  the Cyclone provisioning red it uncovers is issue 1777.
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
  coverage. **Taken 2026-10-09 (D2), recorded in 1627:** kept; tier 2 already
  builds all five unfiltered (`just build tier2` → `just zephyr build-fixtures`
  → `west-fixtures.sh`), and its `coords` stale gate demands them.

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

**Status 2026-10-10:** issue 1509 rules 1 and 3 land as `just check
node-package-invariance` (fast line, ~0.1 s, 3 ratcheted exceptions incl. one
DEBT line); rules 2 and 4 remain, so the issue stays open. Issue 1644 is
resolved.

### W5 — bare metal, the remaining roads

Issue [1512](../issues/1512-c-api-does-not-reach-bare-metal.md) is open for the
**C-rooted** cmake road only: the board has no linker script with `SECTIONS`/
`ENTRY`, no C reset/vector startup, and no C-callable board init (which needs a
new board-seam staticlib, since `nros-c` cannot depend on a board). The six C
roles and a C++ talker exist and boot on the cargo-rooted road (PR #1585); the
other C++ roles are mechanical.

**Acceptance:** 1512 closed, or its C-rooted half recorded as not pursued.

### W6 — board identity residue

Issue [1652](../issues/archived/1652-framework-board-strings-in-names-nuttx-platformio.md)
— NuttX's `qemu-armv7a-nsh` and PlatformIO's `esp32dev` still sit in `names`
the way the Zephyr id did before issue 1519. Needs a typed per-ecosystem field
first.

**Status 2026-10-10:** done — issue 1652 resolved (typed `[board.nuttx]` /
`[board.platformio]`, D5).

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

**In flight at the checkpoint — since closed (2026-10-09):**
- **Issue 1746** — landed as #1808. The configure-time sizes-header heal ran
  before the entity-facts flush re-pointed `<build>/cargo` to its final key,
  so it copied the provisional key's header; the heal is now deferred behind
  the flush (`nros_config_header_heal`). Guarded by
  `check-config-header-single-writer` and case G of
  `tests/cmake-resolved-seed-tests.sh`, both red against the pre-fix tree.
- **First `run-matrix` tier-1 run** — dispatched on `main` at `2b8153621`,
  run 37945434581. Its `tier1` job is the CI acceptance for issues 1684 and
  1685.

**Still open in this phase:** W1 (`fvp_entry` to a generated west app; the
`realtime-c` SMP image id + board; robot1 precedence unit test), W2 (1650),
W3 (tier-1 residue 1686–1688, 1690–1692; 1627; the narrowing-filter
decision), W4 (1509, 1644's lane decision), W5 (1512), W6 (1652), W7 (1535).

## Decisions — 2026-10-09

Taken by the maintainer, each on the recommended option. They narrow what the
work items above still leave open.

- **D1 — issue 1627, how a `west-configure` row proves it built (W3).**
  The row's declared output becomes `build.ninja`, which CMake writes only
  when GENERATE succeeds. This keeps the one rule `west-fixtures.sh` states
  ("the row declares what must exist") and fixes its premise, rather than
  adding an exit-status test beside it. Rejected: checking west's exit status
  (a second rule, and the case the script's comment warns about); documenting
  the hole. Expected consequence: `west_board_import` goes visibly red on the
  idlc provisioning gap, which is real and gets its own fix (provision
  `--rmw cyclonedds` in that lane or gate the row) — not a reason to keep the
  silence. Acceptance as written in 1627: a deliberately broken
  `west-configure` fixture counts FAILED, the genuinely-configuring rows stay
  ok.
- **D2 — the `NROS_ZEPHYR_FIXTURE_FILTER` decision 1627 records (W3).**
  Keep the narrowing: the filter means what it says. Coverage of the five
  west compile checks must come from a lane CHOSEN for it — prove one builds
  them, and if none does, add them to tier 2. Rejected: exempting compile
  checks from the filter (the filter would stop meaning what it says);
  keeping the narrowing with no lane (it re-creates issue 1650's class).
- **D3 — issue 1650, per tree (W2).** `workspaces/launch` gets a row (the
  only end-to-end exercise of launch-v1 composition); `templates/
  multi-package-workspace` gets a build-only row; `templates/
  multi-node-workspace` gets a row (the scaffold embeds it, so a break ships
  to users); `templates/zephyr-byo` gets a RECORDED REASON — it documents a
  bring-your-own-manifest shape, not a build target — and keeps its lint.
  Then the gate: every example root has a row or a recorded reason, keyed on
  the census's shape detection.
- **D4 — issue 1512, the C-rooted bare-metal road (W5).** Not pursued for now,
  and recorded as such in 1512: the cargo-rooted road already boots the six C
  roles and a C++ talker (#1585), and no consumer has asked for a
  CMake-rooted bare-metal project. The remaining C++ roles on the
  cargo-rooted road are mechanical and stay in scope. Revisit on a consumer.
- **D5 — issue 1652, framework board strings (W6).** A typed table per
  ecosystem, on the `[board.zephyr] west_board` precedent:
  `[board.nuttx] board_config` carries `qemu-armv7a-nsh`, its consumer reads
  the field, and the one NuttX row moves to the nano-ros id. `esp32dev` has
  no row; delete it from `names` once a search shows nothing reads it, else
  give it `[board.platformio] board`. Extend `check-deploy-board-resolves`'
  framework-id rule to the new field.
- **D6 — W1 ownership.** Split: the `fvp_entry` migration stays with the
  session on `fix/477-fvp-lane`; this campaign takes `zephyr_entry_robot1`
  (the precedence unit test, then the migration) and `realtime-c` (its own
  image id + the corrected board).

**Resulting order:** D1 + D2 (one PR, on 1627) → W7 (issue 1535, the `mixed`
SEGV — a real bug, not a lane) → D3 (1650) → D5 (1652) and D6's two entries →
D4's recording in 1512 and the mechanical C++ roles. W3's 1684/1685 close on
the tier-1 job of run 37945434581.

**Status 2026-10-10:** D1 + D2 done — issue 1627 resolved and archived (rows
declare `build.ninja`, gated by `validate-compile-checks`; `require_west_fixture`
requires the build stamp; D2 needed no lane change, tier 2 already builds the
five rows). The Cyclone provisioning red D1 exposes is issue 1777.

## Checkpoint — 2026-10-10 (session stopped here)

**Landed / queued since 2026-10-09:** #1847 (1627, D1+D2), #1877 (1652, D5),
#1879 (1509 rules 1 and 3, `node-package-invariance`), #1882 (1650, D3,
`example-build-coverage`), #1885 (robot1 entry generated + precedence test),
#1890 (W5: the five remaining bare-metal C++ roles, all six rows build). Side
fixes: #1849 (1773 watchdog, 1775 provisioning race), #1858 (1781 ament
first-prefix), #1862 (arm-fvp libatomic), #1868 (runner name required).

**Resume here (open draft PRs):**
- **#1892 — realtime-c SMP image** (`[image.zephyr_smp] board =
  "qemu-cortex-a53"`, `entry = "zephyr_entry"`; builds an SMP image). Rebase
  after #1885 merges (both touch issue 1288 and this doc), mark ready, arm.
  Then the last W1 step: migrate realtime-c's hand-written `zephyr_entry`.
- **#1891 — issue 1535 (W7)**: root cause = issue 1566's under-sized C
  publisher storage (560 vs 640 B) corrupting the adjacent Rust heartbeat's
  vtable — already fixed on main; the PR adds a compile-time size guard and
  fixes a second defect it files there (the C++ entry handed Rust the nros-cpp context instead of
  its executor). Remaining: `just ci gate` steps 4–6 and a re-run of the
  `entry_e2e zephyr/mixed/entry_pubsub` cell; if green, archive 1535, ready,
  arm. Unfiled: the mixed Zephyr fixture's probe reports "DEGRADED … examined
  0 input(s)".

**Still open in this phase:** 1509 rules 2 and 4; W1 `fvp_entry` (another
session, `fix/477-fvp-lane`); 1512's runtime lane (entry locator bake).
Tier-1 CI acceptance (1684/1685): run 37945434581's tier 1 was running on
runner `newslab-118-server` at stop; tier 2 failed in preflight — triage with
`just matrix-triage`. Unfiled: the stage reporter labels a runner-lost job
"DID NOT START" (issue-1754 class).

**Infra state:** two self-hosted runners — `nano-ros-runner-newslab-118-server`
and `nano-ros-runner-newslab-133` (this host, tmux `nros-runner`, store
`/mnt/p5plus/aeon/nros-runner`, env `/mnt/p5plus/aeon/runner-env.sh`).
Dependency-pinning work continues in phase 485 (M1–M3 measured; next W1).
