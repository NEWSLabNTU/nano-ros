# Phase 447 — provisioning revision

**Status (2026-09-11). E1, E2 and E3 landed (below) — E3 was the last E item;
other items as marked.** Implements
[RFC-0099](../design/0099-provisioning-is-planned-once-and-prefers-prebuilts.md).
Makes the installed path reach a build, makes a repeated `nros setup` cheap, and
makes "prefer a prebuilt" a rule instead of an intention.

**Prior:** RFC-0097 / phase-443 (release composition), RFC-0095 / phase-440 (the
store is the root), RFC-0062 (one dependency SSoT), RFC-0014 (`nros setup`).

**Source:** six issues from a contained-runner bootstrap — 1264, 1266, 1267,
1273, 1274, 1275 — plus two flaws found reviewing the user and developer steps
against the tree.

## The two numbers this phase exists for

**The installed path dead-ends.** `nros build` resolves its board catalog from a
CHECKOUT; the release asset ships `bin/nros`, the index and `install.sh`. So
`install.sh` -> `nros setup` -> `nros new` -> `nros build` fails on the first
release ever cut. Nobody is hurt today, which is the trap: the only people who
could notice have `NROS_REPO_DIR` from `activate.sh`.

**A repeat costs 153.82 s.** On a fully-provisioned host, `nros setup
mps2-an385-baremetal` takes 153.82 s and `--dry-run` takes 0.01 s. Tools skip on
a provenance marker and clone sources skip on a populated dest; the submodule arm
has no presence check at all.

## Work items

### A1 — the SDK root ships inside the toolchain

Stage `share/nano-ros/{cmake,config,packages}` into the release asset (RFC-0099
D2). Measured payload: **30.5 MB, 2906 tracked files** — use `git ls-files`, not
`du`, which reports 20 G of build output. The three directories named above are
5.9 MB and are NOT the payload; `packages/cli` cannot be excluded whole
(`nros-macros` path-deps two of its crates), the root `Cargo.lock` ships, and so
do `zephyr/`, `scripts/`, `CMakeLists.txt`, `nano_rosConfig.cmake` and the index.

**A second artifact is required and is not the SDK root:** `nros-launch-resolve`.
Every workspace configure goes through it, and it cannot live inside the SDK root
— own cargo workspace, own lock, embeds CPython via pyo3. The release builds it
and stages it beside `nros`; `install.sh` fronts it conditionally, since
`front_newest` refuses an absent declared path and older assets must stay
installable. It links `libpython3.10.so.1.0` as a hard `DT_NEEDED`, so it carries
an ABI floor that D1 owes a declaration for.

*Acceptance:* the asset carries what a build reads; `check-release-manifest`
asserts its presence; the manifest records it. Version-locked to the toolchain
because codegen and runtime are one unit (RFC-0097 D6).

### A2 — the resolution ladder gains a store rung, LAST

`cmd/build.rs` and the scaffolded `CMakeLists.txt` resolve three ways, all a
checkout. Add a fourth: the running toolchain's own SDK root.

*Acceptance:* an `nros` installed from a release builds a scaffolded project with
no checkout present; a contributor inside a checkout still resolves to their
tree, unchanged. **The rung is LAST** — the ownership guard (phase-431 W1)
requires a checkout's `nros` to be that checkout's build, so a store rung placed
earlier silently redirects contributors. Never bake an absolute path: ask the
running toolchain.

### A3 — the clean-host probe reaches the build

`first-project.md` carries zero `probe=` blocks, so no gate runs a user's first
build without a checkout. That is why A1/A2 could ship broken.

*Acceptance:* `just probe bootstrap` runs scaffold -> build -> run in a clean
container with no checkout; it fails before A1+A2 and passes after.

*Status (2026-09-11): the gate landed and is RED — on a defect A1+A2 did not
reach, filed as [issue 1304](../issues/1304-installed-setup-cannot-provision-submodule-sources.md).*

- **What runs.** `just probe bootstrap` runs both front doors; the new
  `installed` track (`just probe installed`) installs a release into a pristine
  container with nothing mounted but the asset, then runs `installation.md`'s
  install + `nros setup` blocks and `first-project.md`'s scaffold and build
  blocks (`track=` tags), then the entry.
- **Where the asset comes from.** No release exists, so the probe builds the
  one `release-nros.yml` would publish — by running that workflow's own
  `run:` steps, extracted from the commit under test
  (`scripts/probe/extract-workflow-steps.py`), on the image its `runs-on`
  names. A probe with its own staging list would be a second definition of
  "what a release contains", green on the day the workflow dropped a line
  (R5's first version). `PROBE_ASSET=<dir>` installs a runner-built artifact
  instead.
- **The two directions, measured.** With A1+A2 the no-checkout / shipped-root
  check passes (`nros sdk-root` answers "via this toolchain's own
  share/nano-ros") and the run stops at `nros setup` — issue 1304: every
  source `native` needs is a submodule, and a release has no gitlink to read.
  With A1+A2 reverted the run stops right after the install, at that check:
  `nros sdk-root` has no answer. Configuring the scaffold by hand on each
  snapshot agrees: reverted, cmake cannot find `cmake/NanoRosWorkspace.cmake`;
  intact, it resolves the release's SDK root, passes Corrosion once a Rust
  toolchain exists, and stops at Cyclone (1304 again).
- **Why the check sits after the install step.** First version checked at the
  END, so both directions died at `nros setup` with the same message — a
  regression of A1+A2 would have been indistinguishable from the open 1304
  red. `extract-book-steps.py --after-step` splices it in where it becomes
  checkable.
- **Still open:** "passes after" — blocked on 1304 (sources, a Rust toolchain
  on the installed path, a configure that finds the installed Cyclone dist).

### B1 — the submodule arm skips what is already there

`git submodule status --recursive -- <path>`: skip iff every line's prefix is a
space. `-` (uninitialised), `+` (differs from the recorded pin) and `U`
(conflict) all mean work. Return the existing `SourceDisposition::AlreadyPresent`,
which already prints `"already present (skip)"`.

*Acceptance:* a second `nros setup <board>` on a provisioned host runs NO git
command — assert that, not the wall time. Check recursively only when
`src.recursive`, or a nested submodule the index never asked for forces work.
Leave the by-SHA fallback alone; it is only reached when the fast path fails.

### B2 — `nros_platform_rmws` stops discarding the parser's stderr

Closes 1264. A python without `tomllib`/`tomli` is currently reported as a
platform that does not exist.

### C1 — smoke runs on the install path, and a ratchet covers its silence

Move `failing_smoke` onto the install path (RFC-0099 D5). Its own doc says
"Absent `smoke` means no opinion, not a pass", so add a smoke-or-reason baseline
that may only SHRINK.

*Acceptance:* a dist that installs but cannot run fails at unpack, naming the
probe and what it printed — not at first use. One broken package must not abort
the other twenty; report at the end.

### C2 — every tool declares a dist or a recorded reason

Survey the 10 source-only tools (RFC-0099 D4). `make` and `ninja` stay `[tool.*]`.
Cross-repo: dist rows are index work, but minting them is `nano-ros-sdk` work.

*Acceptance:* a dist-or-reason ratchet that may only shrink; `play_launch_parser`
published into `nano-ros-sdk` on our own schedule; `espflash` and the other
upstream-publishing tools point at the upstream asset, the pattern
`[tool.zephyr-sdk]` already documents. Fix `[prereq.libpython310]` in passing —
it names a dist that does not exist.

**Measure here:** the cold-bootstrap split between source builds, downloads and
unpacks. E3's whole payoff is a function of it, and C2 is when it changes.

### D1 — a dist declares its floor and is probed before download

*Acceptance:* a host below the floor is refused BEFORE the download, naming the
remedy, and falls back to source; a floor-or-reason ratchet. Note `nano-ros-sdk`
already builds on `ubuntu-22.04` and bundles the ldd closure minus libc, so this
is about interpreter-linked and named deps, not glibc.

**Landed 2026-09-11** (with D2, one branch — both own `sdk_index.rs`).
`DistArtifact.floor` + `orchestration/host_floor.rs`; the refusal is decided in
`plan_install`, so the URL never reaches `execute`. Below the floor with a source
recipe -> `Source { refused: Some(why) }`, printed on the plan line and at
install; with none -> `InstallAction::Refused`, which bails naming the remedy.
Ratchet: `check-dist-floors` + `.config/dist-floor-baseline.txt`.

Floors, MEASURED from each released archive (`scripts/sdk/measure-dist-floor.py`
— ELF version-needs / `DT_NEEDED` / `PT_INTERP`, Mach-O `minos`; parser
cross-checked against `objdump -T` on `xrce-agent`: both say 2.34 / 3.4.30):

| tool | linux-x86_64 | linux-arm64 | macOS |
| --- | --- | --- | --- |
| arm-fvp | glibc 2.17, glibcxx 3.4.26 | — | — |
| arm-none-eabi-gcc | 2.34, 3.4.19 | 2.34, 3.4.22 | arm64 11.0 |
| cargo-llvm-cov | 2.34 | 2.34 | — |
| cargo-nextest | 2.27 | 2.27 | — |
| clang-format | 2.16, 3.4.19 | 2.17, 3.4.19 | arm64 11.0 |
| cyclonedds | 2.34 | 2.34 | arm64 14.0 |
| espflash (C2) | none (static musl) | 2.34 (a `-gnu` build) | arm64 11.0 |
| mdbook | none (static musl) | none (static musl) | arm64 11.0, x86_64 10.12 |
| ninja | 2.15, 3.4.21 | 2.17, 3.4.21 | arm64 11.0 |
| openocd | 2.34 | 2.34 | arm64 14.0 |
| qemu | 2.34 | 2.34 | arm64 14.0 |
| riscv-none-elf-gcc | 2.27, 3.4.30 | 2.27, 3.4.30 | arm64 11.0 |
| sccache (C2) | none (static musl) | none (static musl) | arm64 11.0 |
| verus | 2.34, 3.4.26 | — | arm64 11.0, x86_64 11.0 |
| xrce-agent | 2.34, 3.4.30 | 2.34, 3.4.30 | arm64 14.0 |
| zephyr-sdk (0.16.8) | 2.27, 3.4.21 | 2.27, 3.4.21 | arm64 11.0, x86_64 11.0 |
| zephyr-sdk-1-0-1 | none (self-hosted sysroot) | none (self-hosted sysroot) | arm64 14.0 |

Three measurements that reading would have got wrong:

1. **No dist needs 2.35.** The runner's version is not a binary's floor; every
   `nano-ros-sdk` dist tops out at `GLIBC_2.34`. Ubuntu 20.04 (2.31) is what
   these floors actually refuse.
2. **zephyr-sdk-1-0-1 looked floor-less and is not binary-free.** Its
   `_minimal` archive holds no ELF, but it carries the host tools as a 70 MB
   Yocto self-extractor. Measured from the payload (after `MARKER:`, never run):
   109/108 ELF, every one on the sysroot's OWN `ld.so` + glibc — so `none` is
   the right answer, for a reason the first measurement could not see.
3. **The Zephyr SDK 0.16.8 install mixes both kinds.** 55 of its 1334 dynamic
   ELF are self-hosted (the pokysdk sysroot) and would have inflated the floor;
   the host-linked toolchains alone set 2.27 / 3.4.21. Measured from a local
   install of the same release (`sdk_version` 0.16.8).

**Debt: 0.** The zephyr-sdk 0.16.8 linux-arm64 / macos-arm64 / macos-x86_64
archives (1.0-1.3 GB, upstream) landed first as recorded debt (ceiling 3), and a
follow-up commit measured them and took the ceiling to 0 — the ratchet shrinking
as designed. linux-arm64 measured from the ARCHIVE came out 2.27 / 3.4.21,
identical to the linux-x86_64 figure measured from an INSTALL, which is the
cross-check between the two methods. All three needed a resumed download (curl
92, an HTTP/2 stream reset mid-transfer), which is why the measuring script's
fetch now resumes (`-C -`, `--retry`).

**The ratchet caught its first rows the same day.** C2 (#1273) landed first and
added six dist rows (`sccache`, `espflash`) with no floor; on the rebase
`check-dist-floors` named exactly those six and nothing else. Measured and
stamped in a third commit — `espflash` is the instructive one: its x86_64 asset
is a static musl build (no floor) while its arm64 asset is `-gnu` (glibc 2.34),
so two columns of ONE tool need different answers, which a per-tool floor could
not have said. 49 dist rows across 17 tools, every one floored, debt 0.

**What D1 owes `nros-launch-resolve` (A1's note), and how much of it is paid.**
It links `libpython3.10.so.1.0` (issue 0897) but ships INSIDE the release asset
via `install.sh`, not as an index dist, so `plan_install` never sees it. The
DECLARATION is paid: `[prereq.libpython310]` is now the soname-exact key for that
binary, with `apt = { jammy = [..], noble = [] }` — the "not packaged there"
signal the backward half reads. The ENFORCEMENT waits for `[tool.nros]` to gain
`dist.<host>` rows (phase-431 W5): the ratchet then demands a floor, and its
`system = ["libpython310"]` makes the backward half refuse it on noble before
the download.

**Side finding, not fixed here.** The `arm-none-eabi-gcc` linux-arm64 dist
bundles focal's CPython 3.8, whose `lib-dynload/_ssl` module needs
`libssl.so.1.1`, which jammy does not ship — so `import ssl` fails inside that
gdb's Python on the host it was built for. gdb itself is unaffected. A
`nano-ros-sdk` re-cut question.

### D2 — manager fields gain an OS-version dimension

`apt = [...]` stays valid and means every version; `apt.noble = [...]` overrides.
Fixes `libpython310` and the t64 renames.

**Landed 2026-09-11.** Spelled `apt = { default = [..], noble = [..] }` — TOML
cannot hold `apt = [..]` beside `apt.noble`, so the table carries its own
default (RFC-0099 D9 records why). `ManagerPackages` is the Rust type;
`scripts/lib/index_packages.py` the one Python reader (two scripts moved onto
it). An explicitly empty release list means "not packaged there", and D1 reads
it. `libssl3` gains `noble = ["libssl3t64"]`. `libpython310` was MISNAMED: its
only consumer is a SOURCE build, which links the host's own CPython, so it is
`[prereq.libpython3]` now (`jammy = libpython3.10`, `noble = libpython3.12t64`,
`default = libpython3-dev`) with a minor-independent probe. Every noble name was
checked against the archive, not assumed — both `libssl3` and `libpython3.12`
turned out to be t64 renames there.

### D3 — a pinned rosdep snapshot, as a fallback rung

Vendored index data, `provider = "system"` only, keys from it marked unprobed
(RFC-0099 D8). Not the rosdep tool, and not consulted from the host.

*Acceptance:* `<depend>libopencv-dev</depend>` in a user's own package resolves;
the same tree resolves the same way on two machines.

### E1 — `--tool` becomes repeatable

`--source` already is. Merge the adjacent call sites (`workspace.just:64-65`
ninja+make, `:693-694` nextest+llvm-cov).

**Landed.** `--tool` is `Vec<String>`; both call-site pairs are one invocation.
`--prefix` still places ONE tool and refuses two. `--tool a --tool b --check`
asks each and fails if any is off its pin. `nros setup board`'s (a2) loop, which
loaded the index and wrote the lock once PER TOOL, is one plan too.

### E2 — the session's plan is resolved once

One plan before any fetch; one apt ask for the union (closes 1274); one lock
write; one index read.

*Acceptance:* a bootstrap prints ONE `apt install` line, not three overlapping
ones with different subsets. Still no sudo by default.

**Landed** — `cmd/setup/session.rs`. Every install path (`nros setup <board>`,
`--tool …`, the lazy `ensure_tools`) builds a `SessionPlan` first — every
`plan_install` and ONE `SystemAsk` over the union of its tools' `system = [..]` —
then `run_sequential` executes it and `SessionRun::finish` is the lock's only
writer. Resolution takes an injected probe, so it stays pure (issue 0374).

Three findings changed the fix from "batch the ask":

* **The subsets differed because the first ask was WRONG, not because it was
  early.** Its extra `make ninja-build` were keys the same session then
  provisioned from the store — and on Ubuntu 22.04 neither apt package meets
  its version floor, so the ask could never be satisfied. A chain key
  (`providers = ["system", "sdk"]`) that is missing is now `PrereqState::Store`:
  offered as `nros setup --tool …`, never as an apt package, and not asked at
  all when the plan installs it. `[prereq.make]` gained the chain ninja already
  had — its "diagnosis only" note named one blocker, tarball sources, which
  `[tool.make]` has since removed.
* **`--system`'s print mode ignored `--role`** — it re-read `index.prereqs()` —
  and ran each entry's bare probe rather than the chain. It now shares one
  classification (`classify_prereq`) with `--check` and the install paths.
* **Across processes the ask is deduplicated by a ledger, not merged.** RFC-0099
  D6 keeps one process per platform verb, so `NROS_SETUP_SESSION` names a file
  the DRIVER opens (`scripts/lib/setup-session.sh`, used by `just setup` and
  `runner-provision.sh`; a nested driver joins). A key asked once is not asked
  again; a key nobody asked still is. Unset, nothing changes; `--sudo` never
  reads it.

*Test gap C1 left, closed:* `a_board_install_smokes_every_package_and_a_broken_one_stops_nothing`
drives the real executor over `file://` dists. Mutation — `run_sequential`
calling `sdk_store::execute` instead of `execute_and_probe` — turns it red at
`smoke.passed`, and C1's own e2e test with it.

*For E3:* replace `run_sequential` and nothing else. The four ordered things are
properties of the types — `finish` is the only lock writer and walks plan order;
a plan holds one step per package (so `front_newest` never races itself);
`bin_dirs()` is derived from step order; `OrderedLog` releases a step's lines
only after every earlier step closes, and is tested with out-of-order
completion. Each step owns its `SmokeFailures`; `finish` folds them in plan
order. One thing E3 must still take care of: `sdk_store::execute` prints its own
progress directly, outside `OrderedLog`.

### E3 — the plan executes as a pipeline, bounded by CPU count

Closes 1266 and 1267. Fetch, verify and unpack overlap across packages; progress
is visible for a long download.

*Acceptance:* four things stay ordered — the lock's single writer, `front_newest`
(issue 0500's newest-first rule, where a stale entry shadowing a fresh one prints
success on BOTH paths), `bin_dirs` PATH order, and per-package output flushed in
plan order.

**Landed** — `run_pipelined` replaced `run_sequential`, and nothing around it
changed. A worker pool over the resolved plan, as many workers as the host has
CPUs, overridable with `nros setup -j/--jobs N` or `NROS_SETUP_JOBS`; zero or a
non-number is refused by name. The pool owns the order of nothing — the four
ordered things stay where E2 put them — and owns two properties of its own: no
step is dropped or run twice, and a step that fails or PANICS is its own outcome.
`SessionReport::bin_dirs`, folded by `finish` beside the lock, is what the
CMakePreset `PATH` and `ensure_tools` now read.

* **The gap E2 named is closed.** `orchestration/step_log.rs` installs a
  per-step sink on the worker thread: `sdk_store`'s three `eprintln!`s go
  through `step_log::say`, and every child it spawns (`curl`, `tar`, `git`,
  `configure`, `make`) has its stdout/stderr routed line by line into the same
  plan-order log. With no sink both behave exactly as before, so `sdk_store`
  kept its signatures.
* **Two lanes, not a smaller pool.** `[tool.*]` source builds run one at a time
  (each already uses the whole machine), and so do `[source.*]` steps (the
  submodule arm takes the superproject's `index.lock` and rewrites
  `.git/config` — a hazard the plan's one-step-per-package does not cover). The
  scheduler hands out the EARLIEST step whose lane is free, so a waiting build
  never idles a worker a download could use.
* **Progress for a long download** (1266): the two `curl` copies are one
  `download` helper; curl stays silent (its bar is a carriage-return repaint,
  one long line in a CI log) and `step_log::watch_download` prints a plain
  byte-count line after 10 s and every 30 s after, then a closing line. A short
  fetch prints nothing. `cmd/setup.rs::fetch_index` stays silent on purpose.

*Tested under concurrency*, completion forced out of plan order by handshake
(`StepCtx::wait_closed`) and asserted on order and outcome, never on time; each
test also asserts the completion order it forced, so "in plan order" is a claim:

| property | test |
| --- | --- |
| output (incl. child stdio and `step_log::say`), lock never written by a step, `bin_dirs`, error + smoke fold | `under_concurrency_every_ordered_thing_leaves_in_plan_order` (completion `[3,2,1,0]`) |
| one broken package stops nothing; each step runs exactly once | `a_failing_or_panicking_step_stops_none_of_its_siblings` |
| lanes never overlap; the rest flow around a waiting build | `steps_that_share_a_lane_never_overlap_and_the_rest_flow_around_them` (completion `[2,0,1,3,4]`) |
| real path at concurrency 3: each package's lines whole, in plan order | `the_real_install_path_prints_each_package_whole_and_in_plan_order` |
| C1/E2's board-path test, now at concurrency 3 | `a_board_install_smokes_every_package_and_a_broken_one_stops_nothing` |

*Mutation pass* (each applied, the session + step_log tests run, source restored):

| mutant | result |
| --- | --- |
| fold (lock, smoke, errors) in completion order | RED |
| record ONLY the lock in completion order | survives — **equivalent**: `SdkLock` is a map keyed by tool name and a plan holds one step per name, so recording order is unobservable in the file |
| derive `bin_dirs` from completion order | RED |
| `OrderedLog` releases a step's lines as it finishes | RED |
| `OrderedLog` streams every step live | RED |
| a failing step aborts its siblings | RED |
| no lanes | RED |
| scheduler blocks on a held lane in plan order | RED (the deadlock guard) |
| child output bypasses the step sink (E2's gap reopened) | RED |
| a long download never reports progress | RED |
| install without the smoke probe (C1's) | RED |

The pass also caught two defects in the first draft, which surfaced as one hang
(read from the stuck test binary's thread names): a download test raced its
watcher's first tick, and a `fetch` that panicked then DEADLOCKED the watcher,
because the stop sender outlived the scope that joins it. Both are fixed and
tested. Looking for the same class found a third: a child that leaves a
background process holding its pipes (a build server) would have hung the step
on EOF, where inherited stdio never did — so the drain after a child exits is
bounded, and says when it gave up.

### F1 — the Zephyr module set moves under the index — **LANDED 2026-09-11**

Closes 1275. Makes board -> module the same mechanism as board -> package, so
`--dry-run` can price the 2.5 GB of HALs.

*Acceptance:* prove it rather than assume it — grep the conf tree and build one
native_sim and one mps2_an385 leaf against the narrowed manifest. A board
fragment can pull a module with no fixture naming the board (issue 0876's shape).
Hold `hal_espressif` back deliberately: whether Zephyr's espressif support makes
our ESP-IDF path a duplicate is a question to measure, not to settle here.

**What landed.** `[zephyr_module.*]` is the SSoT (`why` / `needed_by` /
`approx_mb` / `lines`); a module is in `west.yml`'s allowlist iff its `lines`
carries `"3.7"`, and `west-4.4.yml`'s iff `"4.4"`, asserted BOTH ways by
`check-zephyr-module-allowlist`. The manifests stay COMMITTED rather than
generated — `west init -m <url>` reads `west.yml` from a bare clone, before any
`nros` exists to generate one. `hal_nxp`/`hal_stm32`/`hal_nordic` left the
allowlist: **~2.29 GB off a fresh `west update`**, priced by `nros setup zephyr
--dry-run`.

**Three things the acceptance found that reading would not have.**

1. The conf-tree sweep came back EMPTY — zero vendor-HAL-pulling CONFIG symbols
   anywhere. The 0876 shape it was guarding against did not occur.
2. The board inventory is SIX, not the three `fixtures.toml` names:
   `qemu_cortex_m3`, `qemu_cortex_a9` and `fvp_baser_aemv8r` are built too. All
   `cmsis`-or-nothing, so the conclusion holds — but a fixture manifest is not
   the board inventory.
3. **`mcuboot` would have been a plausible fourth deletion on a grep, and would
   have been wrong.** Its consumer is SYSBUILD, not an `#include`;
   `<build>/sysbuild_modules.txt` names it. This is why acceptance was a build.

`hal_nxp` has a real consumer that is not in this tree — `mr_canhubk3/s32k344`,
the downstream safety-island board ~40 in-tree comments cite. Its entry names
the board and states the remedy: a downstream re-enables it in its OWN
manifest's allowlist, because an `import:`ed manifest composes rather than
inheriting ours as a ceiling.

`hal_espressif` was held back as instructed, and the index says plainly that it
has NO measured consumer — questions (1) and (2) of 1275 both came back empty.
Deleting the line would settle the platform-strategy question by accident, so it
stays and the question is **issue 1282**. 275 MB of the 2.5 GB is knowingly
still paid.

*Build evidence, and its limit.* Proven in a topdir where the three HALs are
absent from disk as well as from the manifest (`west list` returns 10 projects,
not 13). `c/talker` on `native_sim/native/64` **links** — `zephyr.exe`, 12.2 MB.
`c/talker` on `mps2_an385` resolves its modules with `cmsis` and no vendor HAL,
and clears Kconfig, devicetree and the whole C module compile, but does not
reach a link: it stops in `nros-cpp`, which calls `Executor::wake_raw_ptr`
(gated `all(alloc, rmw-cffi)`) from a function gated on `rmw-cffi` alone — a
pre-existing feature gap on `main`, in a file this branch does not touch.
`rust/talker` is blocked by a second pre-existing break, the executor-sizing
const assert (the #1172 recurrence); a CONTROLLED run — identical topdir, all
four HALs present, ORIGINAL manifest — fails identically, which is what
attributes it away from the manifest change. **So the mps2 half of the
acceptance is module-resolution-proven, not link-proven.**

*Methodology note worth keeping.* Symlinking `zephyr` into a test topdir does
NOT work: Zephyr resolves the west topdir from `ZEPHYR_BASE`'s REAL path, so the
first build silently used a different workspace's unnarrowed manifest and proved
nothing — `zephyr_modules.txt` is what caught it. `cp -al` gives real
directories at no disk cost, and is safe because git never writes in place.

## Checkpoint (2026-09-11)

Written so the campaign can be resumed on another machine. Everything below is on
the remote; the one thing that is NOT — a poller on the original host — is named
where it matters.

### Merged

| PR | item | result |
| --- | --- | --- |
| #883 | RFC-0099 + this phase | — |
| #890 | B1 submodule fast-skip | `nros setup mps2-an385-baremetal` 153.82 s -> ~0.5 s |
| #891 | B2 parser stderr | closes 1264 |
| #895 | D3 pinned rosdep snapshot | 1788 keys, `provider = "system"` fallback, unprobed |
| #896 | A1+A2 SDK root ships; store rung LAST | 30.5 MB / 2906 files + `nros-launch-resolve` |
| #906 | F1 Zephyr modules under the index | closes 1275 |
| #909 | payload correction + issue 1294 | — |
| #911 | leaf-target gate prunes `target-*` | 27 min -> 0.5 s |
| #914 | C2 dist-or-reason ratchet | 15 -> 17 of 25 tools carry a dist; closes 1273 |
| #918 | C1 smoke on the install path | 14 of 25 tools declare a `smoke` probe, the other 11 a reason. Left the queue twice, neither time for a defect in C1 (see findings) |
| #925 | A3 clean-host probe | red nightly until #1304 lands, and it fails at a different step than a #896 regression would |
| #930 | lane-skip needs an interpreter | the L3 fix: `provision-zenohd` rc 78 on the container runner |
| #931 | D1+D2 floors + OS-version dimension | 49 floors measured, none guessed; no dist needs glibc 2.35 |
| #932 | `no-find` gate sees `dir/**` | `config-header-single-writer` 37 min -> 1 s |

### In the merge queue or armed

| PR | item | note |
| --- | --- | --- |
| #927 | E1+E2 session plan | closes 1274. Re-armed by the poller once #918 landed; rebased onto main, 2 -> 1 commit |
| #933 | E3 pipelined executor | closes 1266 and 1267. Rebased 4 -> 1: C1 and its fix, then E1+E2 itself, all dropped by patch-id once they landed on main. Armed |

### In flight at this checkpoint

| work | branch | state |
| --- | --- | --- |
| #1304 (A4) | `work/1304-installed-setup-provisions-without-a-checkout` | the agent stopped at the API session limit mid-change. Its tree is pushed as-is: A3's probe commit, two `wip(#1304)` commits (unreviewed, untested), and a fix to `cargo-target-spelling`'s no-triple arm — the WIP's rustup fallback defeated that PATH-only negative control, which is what first refused the push. No PR yet |

### What is left

- **#1304 (A4)** — the installed path dead-ends at `nros setup`: submodule pins read from a git history a release does not have, no Rust toolchain provisioned, CMake cannot find the Cyclone package. The phase's headline acceptance — an installed `nros` builds a scaffolded project — depends on it, and so does A3's pass direction.

### Open findings, not fixed here

- The **arm64 `arm-none-eabi-gcc` dist's bundled Python `_ssl` needs `libssl.so.1.1`**, which jammy does not ship — `import ssl` fails inside that gdb. Needs a re-cut in `nano-ros-sdk`.
- **`nros-launch-resolve`'s floor is declared but not enforced** until `[tool.nros]` carries dist rows (phase-431 W5).
- **`nros::init::ros_args_refusal_tests`** reads `ROS_DOMAIN_ID` with no lock while `env.rs`'s tests mutate it under `env_lock()`, in the same binary. A flake, observed once under a full parallel run.
- **`just queue-triage` looks back 15 runs, not a time window**, so a minutes-old ejection had already scrolled out of it on a busy queue. #918's was found with `gh` directly.
- **Ratchet collisions between two correct PRs.** Three ratchets now guard `[tool.*]` (dist-or-reason, smoke-or-reason, dist-floors). Two PRs each green alone can be ejected together when the queue combines them; carrying the parent does not prevent it. Satisfy all three when adding a tool.
- **A force-push did not fire `pull_request`.** #918's fixed head received only `arm auto-merge`; `gate.yml` has a bare `pull_request:` trigger, yet no `synchronize` run appeared, so there was no `CI` context and the queue dropped the PR with no failing run anywhere to read. Close/reopen fired `reopened` and ran it. After force-pushing a PR, confirm its required check actually ran on the new head before trusting auto-merge. `workflow_dispatch` is not a substitute — it runs the full lane, including `check-build`, which cannot pass in CI.
- **Subagents are not woken by a watch or monitor.** Two ended their turns waiting on one. Briefs say to block with `timeout 590 tail --pid=<pid> -f /dev/null`.

### Resuming on another machine

1. Clone `nano-ros` and read this section. Every PR above is on the remote.
2. **#927 is already re-armed** (the poller fired when #918 landed). Nothing to do unless the queue ejects it; `just queue-triage` or `gh pr view 927` says.
2a. **Resume #1304:** check out `work/1304-installed-setup-provisions-without-a-checkout`, read the newest `wip(#1304)` commit (its diff is where the agent stopped), finish, squash the two `wip` commits, then `just ci gate`, open the PR and arm it. `just probe` via A3's clean-host probe is its acceptance: the probe must go green.
3. `nano-ros-sdk`: branch `work/phase-447-c2-play-launch-parser-dist` (`9babb72`) is pushed. Fast-forward it to `main` and cut a release to seed the `play_launch_parser` dist; until then it stays on the dist-or-reason list.
4. A3's revert-direction mutant was not pushed — it is derived, not work. #896
   landed as TWO commits through the rebase-merging queue (no merge commit), so
   revert both, newest first:
   `git checkout -b probe-mutant-revert-896 origin/main && git revert --no-edit e4f3d5add04d~1..434b121aeffa`
   (e4f3d5add04d is A1+A2 itself; 434b121aeffa is its follow-up clippy fix. Reverting only
   the newer one leaves the feature in place and proves nothing.)
5. Local-only branches on the original host were NOT pushed; they are scratch from other sessions, not this campaign. The largest is `split/config-surface` (21 commits); others include `integrate/phase-206-w5`, `rb10`, `local-pre-rebase`, `pr48`. If any matter, push them from that host.

## Order, and what collides

`cmd/setup.rs` is touched by C1/E1/E2/E3 and `sdk_store.rs` by B1/D1/E3. Group by
FILE, not by theme, or they collide.

| wave | items | note |
| --- | --- | --- |
| 1 | **A1+A2** (one agent) · B1 · B2 · D3 | disjoint |
| 2 | A3 · C1 · C2 · F1 | A3 needs wave 1 |
| 3 | **D1+D2** (one agent, `sdk_index.rs`) · **E1+E2** (one agent, `cmd/setup.rs`) | |
| 4 | E3 | needs E2's plan structure and B1's skip |

A1+A2 are one agent because they share the `share/nano-ros/` layout; split, they
would negotiate it by guessing.

## Non-goals

* `nros run` / `nros flash` — RFC-0097 D8.
* An acceptance range for `NROS_CODEGEN_VERSION` — RFC-0097 D6.
* rosdep's YAML syntax, or rosdep as a runtime resolver — RFC-0099 D8.
* A wider `host_key` — RFC-0099 D5.
* A session cache with a TTL — RFC-0099 D6.
* Collapsing `just <platform> setup` into one aggregated call — RFC-0099 D6. The
  fast-skip makes it unnecessary, and the per-platform recipe is the command a
  USER would run.

## Issues homed here (survey 2026-09-11)

Every open issue was checked for a home phase; these had none, or were mentioned
here only in passing. A mention is not an owner — an issue with no work item is
an issue nobody is accountable for. Each row is a work item: the issue holds the
evidence, the item is *close it*.

| issue | why it belongs here |
| --- | --- |
| [#1259](../issues/1259-nros-setup-tool-leaves-zephyr-sdk-without-toolchains.md) | `nros setup --tool zephyr-sdk-1-0-1` reports success and leaves an SDK that cannot run: the index pins the minimal bundle with no post-install. This is C1's 'installs but cannot run' and C2's dist-or-reason, and neither names the id |
| [#1262](../issues/1262-nros-setup-lock-skips-present-tools-and-follows-cwd.md) | `--tool` records a tool in `nros-sdk.lock` only when it installs, and follows the cwd. E2's 'one plan, one lock write' is the stated fix |
| [#1273](../issues/archived/1273-prefer-prebuilt-dist-over-source-build.md) (RESOLVED 2026-09-11, archived upstream while this table was being written) | tools build from source because the index has no `dist` row. C2 IS this issue's fix; the item should name the id so closing one closes the other |
