---
id: 1038
title: "nightly triage (phase-413 W2.3): three of six cell failures are one class — the platform job builds a lane whose prerequisites its own setup never installs; none is a product regression"
status: open
area: ci, build, testing
severity: medium
found: 2026-09-04
related: [1030, 0463, 0937, 1006, 1007, 0996, phase-413, RFC-0062]
---

# The job does not provide what its own lane consumes

**Filed under phase-413 W2 item 3**, which owns exactly this — *"`nightly` —
failure ×4 … use `just nightly-triage`"* — and whose acceptance says *"A lane
that cannot be made green is a finding, not a skip — file it."* This is that
finding. Nothing here proposes a new mechanism; each instance maps to a wave
that already owns its fix.

Triage of nightly run `33831829557` (dispatched 2026-09-04T03:04Z against a
current tree, deliberately — the previous run predated #245 and its `nuttx`
cell was reproducing a fixed bug). Six failing jobs, **zero product
regressions**. Three share one cause.

## The class

`nightly.yml`'s `platform` job runs, in order:

    just ${{ matrix.plat }} setup
    nros setup --source px4-rs --source nuttx-libc
    just ${{ matrix.plat }} build-all

`build-all` reaches artifacts that neither `setup` step installs. Each instance
then fails in the vocabulary of whatever it reached first, which is why they
read as three unrelated bugs.

### 1. `nuttx` — a cross toolchain

```
NROS_LANE_NAMED_FAIL: nuttx: riscv-none-elf-gcc not found (run: nros setup qemu-riscv-nuttx)
error: you named `nuttx`, so this is a FAILURE, not a skip.
```

The arm leaves built; the riscv ones need a toolchain `just nuttx setup` does
not install. **The lane behaved correctly** — phase-411's named-platform rule
fired exactly as designed, loudly and at the point of decision. The defect is
upstream of it.

### 2. `threadx_riscv64` — a generated cargo config

```
error: failed to parse manifest at `examples/qemu-riscv64-threadx/rust/talker/Cargo.toml`
Caused by: could not load Cargo configuration
Caused by: failed to load config include `../../../../../nros-patch.toml`
```

Issue 0463 verbatim, and worth restating because it does not look like a
provisioning failure: the leaf's TRACKED `.cargo/config.toml` includes the
central `nros-patch.toml`, which is generated and gitignored. A missing
`include` target is a hard cargo error during MANIFEST PARSE, four frames deep,
naming neither `nros sync` nor the leaf. The job never runs sync or `_codegen`.

The same job logged the softer form of the same gap dozens of times:

```
nros build: warning: no selection facade at .../generated/nros-selection/nuttx_entry
  — building `nuttx` without its RMW and ROS edition. Run `nros sync` first (issue 0937).
```

A warning where the leaf tolerates it, a hard parse error where it does not.

### 3. `esp32` — apt packages for a source build

```
nros setup: 1 package(s) have no prebuilt for linux-x86_64 — BUILDING FROM SOURCE: esp32-qemu
Error: nros setup --tool esp32-qemu: needs 3 system package(s) this host is
missing: libglib2-dev, libpixman-dev, libgcrypt-dev.
```

Adjacent to issue 1006 but not the same: 1006 is about esp32-qemu's RUNTIME
dependency set being a property of the machine that built it; this is a BUILD
prerequisite absent from the container image. Related work is in flight
(phase-413 W3 — the index as SSoT for OS packages).

## Where each fix already lives

**esp32 is not a missing mechanism — it is W3's conversion, unapplied.** The
index ALREADY declares all three packages, twice over: as `[prereq.libglib2-dev]`
/ `[prereq.libpixman-dev]` / `[prereq.libgcrypt-dev]` with apt mappings and
`pkg_config` probes, and in `[tool.esp32-qemu]`'s own `system = [...]` list. The
error names them *because the index knows them*. `nros setup --system` resolves
that closure (RFC-0062 / phase-327; measured 38 present, 5 missing on a dev
host). The nightly job never runs it.

Phase-413 W3 is written as *"no workflow installs what the index already
declares"* and converts `docs.yml` and `nightly.yml`'s clang step to
`nros setup --system --sudo`. This is the same conversion, one step over: the
workflow installs nothing, and the thing it does not install is indexed.

**Checked against the gate as IMPLEMENTED, not as specified**
(`scripts/check-workflow-indexed-apt.py`, landed 2026-09-04): it matches
`apt(-get)? install` lines and refuses one that names a `[prereq.*].apt`
package. That is duplication — a workflow restating the index — and it is the
right rule. It has no way to express the opposite, and this site apt-installs
nothing at all, so the gate is silent here by construction. Widening W3's
acceptance to cover omission is a separate decision for whoever owns the wave.

**nuttx and threadx_riscv64 are W2's "fix the lane" work**, and both are one
missing invocation rather than a design gap: `nros setup qemu-riscv-nuttx` for
the toolchain (the failure text names the exact command), and a sync/`_codegen`
step for the leaves whose tracked `.cargo/config.toml` includes a generated file.

**None of this is W6.** W6 (rosdep parity — `package.xml` as the dependency SSoT)
is explicitly RFC-first and *"must not be started as an implementation task"*.
It is the right long-term home for "a workspace states its own needs", and it
would subsume the per-lane knowledge these three failures encode. It does not
need to land for any of the three to be fixed, and none of them should be used
to justify starting it early.

## The gate boundary, stated

Issue 1030 gated the adjacent shape — a workflow step whose events do not cover
its consumers' — by reading the justfile's own dependency ORDER
(`_codegen: setup-launch-resolve generate-bindings …`). None of the three above
is declared that way: they are `nros setup --tool` / `--system` invocations and a
missing sync, and the justfile orders none of them before `build-all`. That rule
reports how little it covers (1 invocation today), which is what makes this
boundary measurable rather than a surprise. Widening it is a candidate, not a
proposal — `required_producers` already reads a recipe's hard-failing remediation
text, and `run: nros setup qemu-riscv-nuttx` is exactly that shape in a different
spelling. Whether that belongs here or inside W6's scope resolution is W6's
question to answer, not this issue's.

## The other three failures, for completeness

* **`freertos`** and **`threadx_linux`**: `Real failures: 0 / 0`. All 9 e2e
  cases skipped on an unmet precondition, and `_check-skip-budget` correctly
  refused to call that a pass ("This lane verified nothing"). The skip reason is
  TRUNCATED at the same column in both the log and the junit
  (`[SKIPPED] freertos:`), so it was traced in source instead: all nine skip at
  `rtos_e2e.rs:514` via `require_e2e()`, and the only precondition the two
  platforms share is `zenohd_unavailable_reason()`. Both jobs do
  `source /opt/ros/humble/setup.bash`.

  **CONFIRMED 2026-09-04** — the image has NO zenoh packages at all. The
  hypothesis was that it lacks `ros-humble-rmw-zenoh-cpp`; it does, and the
  chain is now closed rather than merely plausible:

  * `ci/docker/ci-base/Dockerfile` builds `FROM ros:humble-ros-base`, which does
    not carry that package, and its apt list adds `ros-humble-example-interfaces`
    and nothing else ROS-side.
  * Queried a built image (`nros-ci-test:humble`, 2026-08-28, from this
    Dockerfile): `dpkg -l | grep -i zenoh` returns NOTHING. The only RMW
    implementation present is `ros-humble-rmw-fastrtps-cpp`.
    `/opt/ros/humble/lib/rmw_zenoh_cpp/` does not exist, and a `find` over
    `/opt/ros` for `rmw_zenohd` or `libzenohc*` returns nothing.

  So: no router -> `zenohd_unavailable_reason()` returns `Some` ->
  `require_e2e()` fails -> all 9 cases `skip!` -> `_check-skip-budget` reports
  "this lane verified nothing". Every language and variant, both platforms,
  which is the uniformity that pointed at a shared precondition rather than a
  code fault.

  (Scope of the check: the Dockerfile is the SSoT for what the image contains,
  and a local build of it agrees. The PUBLISHED `ghcr.io/newslabntu/nano-ros-ci:humble`
  was not pulled — 5.7 GB — so the claim rests on the recipe plus a build of it,
  not on the exact published layer.)

  **The fix is TWO changes, not one, and the Dockerfile already predicts the
  second.** Installing `ros-humble-rmw-zenoh-cpp` makes the router RESOLVE but
  not necessarily RUN. That image sets `AMENT_PREFIX_PATH` / `LD_LIBRARY_PATH`
  as static `ENV` — a snapshot of what sourcing `setup.bash` produced for the
  CURRENT package set — instead of sourcing it. `rmw_zenoh_cpp` installs into
  `<prefix>/opt/zenoh_cpp_vendor/lib`, which that snapshot does not name, and a
  router paired with the wrong `libzenohc.so` SEGVs mid-startup rather than
  failing to load. That is issue 0774 exactly, and the Dockerfile's own comment
  names it: *"a ROS package that installs to a DIFFERENT prefix ... would need
  this block updated -- `ros-humble-rmw-zenoh-cpp` adding
  `<prefix>/opt/zenoh_cpp_vendor/lib` to `LD_LIBRARY_PATH` is the case issue
  0774 is about. Re-run the capture command above after changing the ROS package
  set rather than assuming."*

  Read in hindsight, that comment was the tell: it names the package
  HYPOTHETICALLY, as something that WOULD need handling — which is only how you
  write it when the package is not installed.

  Worth noting separately: a skip message that carries the remedy is useless if
  the transport truncates it. Both artifacts cut it at the colon.

* **`tier 2 nightly`**: three fixture families missing `.inputsig` on the
  self-hosted runner; `_lane-gate` refused. Runner state, not code. It did get
  PAST `runner-doctor` this run — the label failure that killed it on
  2026-09-03 is gone.
