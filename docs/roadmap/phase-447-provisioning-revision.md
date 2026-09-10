# Phase 447 — provisioning revision

**Status (2026-09-11). All work items open.** Implements
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
D2). Measured payload: **5.9 MB, 2869 tracked files** — use `git ls-files`, not
`du`, which reports 20 G of build output.

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

### D2 — manager fields gain an OS-version dimension

`apt = [...]` stays valid and means every version; `apt.noble = [...]` overrides.
Fixes `libpython310` and the t64 renames.

### D3 — a pinned rosdep snapshot, as a fallback rung

Vendored index data, `provider = "system"` only, keys from it marked unprobed
(RFC-0099 D8). Not the rosdep tool, and not consulted from the host.

*Acceptance:* `<depend>libopencv-dev</depend>` in a user's own package resolves;
the same tree resolves the same way on two machines.

### E1 — `--tool` becomes repeatable

`--source` already is. Merge the adjacent call sites (`workspace.just:64-65`
ninja+make, `:693-694` nextest+llvm-cov).

### E2 — the session's plan is resolved once

One plan before any fetch; one apt ask for the union (closes 1274); one lock
write; one index read.

*Acceptance:* a bootstrap prints ONE `apt install` line, not three overlapping
ones with different subsets. Still no sudo by default.

### E3 — the plan executes as a pipeline, bounded by CPU count

Closes 1266 and 1267. Fetch, verify and unpack overlap across packages; progress
is visible for a long download.

*Acceptance:* four things stay ordered — the lock's single writer, `front_newest`
(issue 0500's newest-first rule, where a stale entry shadowing a fresh one prints
success on BOTH paths), `bin_dirs` PATH order, and per-package output flushed in
plan order.

### F1 — the Zephyr module set moves under the index

Closes 1275. Makes board -> module the same mechanism as board -> package, so
`--dry-run` can price the 2.5 GB of HALs.

*Acceptance:* prove it rather than assume it — grep the conf tree and build one
native_sim and one mps2_an385 leaf against the narrowed manifest. A board
fragment can pull a module with no fixture naming the board (issue 0876's shape).
Hold `hal_espressif` back deliberately: whether Zephyr's espressif support makes
our ESP-IDF path a duplicate is a question to measure, not to settle here.

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
