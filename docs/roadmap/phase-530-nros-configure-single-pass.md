# Phase 530 — `nros configure`: one resolver for every road, tight sizing, and no fixed point

**Status (2026-11-03). OPEN — W0 done (fallback not taken), W1 landed (with W9 step 1), W2 landed, W4 landed (on the W2 branch; W3 is a sibling), W5 landed (on W4; the NuttX make road is a guest road, Ruling R36), W6 landed (on W5; every hooked road re-takes its census in the build, issue 2237 resolved).** Implements
[RFC-0120](../design/0120-nros-configure-one-resolver.md) D1 to D7, D9 and
the driven and hooked rows of D8. Meets
[RFC-0094](../design/0094-resolve-before-configure.md) A3 as originally
written: the fixed point is deleted.

Builds on:

- [phase-439](archived/phase-439-resolve-before-configure.md) (archived), which
  landed the resolve phase, the W6 bounds seed, the W7 entity take and the W8
  promise check;
- issue 2194 (resolved), the board knob rung.

---

## Why

Phase-439 made a host configure single-pass only when `nros build` started it.
Four configures still converge through `NanoRosReconfigure.cmake`'s forged
future timestamp, the mechanism behind issues 0940, 0991, 1002 and 1119. And a
plain `west build` of a leaf is not tightly sized at all: its entity inventory
does not compose (the talker's `system.toml`, phase-481 W3), so it falls back
to crate defaults, which issue 2202 measured at ~400 KB on a 520 KiB part.

The maintainer's requirements (2026-11-03):

- `west build` and `nros build` are both tightly sized and always see the
  latest knobs;
- `nros build` is the main path and works like cargo;
- other roads are supported where a user's tool needs them;
- a host compiler is an accepted requirement.

## Work items

### W0 — Spike: the census digest as a host-configure dependency (DONE 2026-11-03)

RFC-0120 D7 depends on one behaviour. A build edge writes
`<build>/nros/census.digest` write-if-changed (`restat = 1`). That file is in
`CMAKE_CONFIGURE_DEPENDS`. Ninja should then:

- bring the edge up to date before deciding whether `build.ninja` is stale;
- re-run the host configure only when the digest's bytes changed.

Build a five-line CMake + ninja project (cmake 3.22, the tree's pin) and
measure three things:

- a source edit that leaves the digest unchanged → no `Re-running CMake`;
- an edit that changes it → exactly one;
- no "multiple rules generate" or dependency-cycle error.

Repeat on a Zephyr native_sim leaf.

- **Acceptance:** the three measurements recorded in this doc, both projects.
- List which entity-derived values are Kconfig symbols, because D7's
  build-time fallback cannot reach `.config`.
- **Decision step:** if any measurement fails, take RFC-0120 D7's fallback.
  Entity-derived knobs move to build-time delivery (a generated header, plus a
  cargo `--config` file with ninja edges), and W6 becomes that work. Record the
  choice in RFC-0120.

**Results (measured 2026-11-03).** The spike: a build edge (inputs = source)
writes `census.digest` write-if-changed, as a BYPRODUCT so ninja restats; the
digest file is in `CMAKE_CONFIGURE_DEPENDS`; configure also writes the digest
itself.

Plain CMake 3.22.1 + ninja 1.13.2 project:

| test | re-runs | configures | digest |
| --- | --- | --- | --- |
| T1 clean build | 0 | 1 | pubs=1 |
| T2 no-op | 0 | 1 | pubs=1 |
| T3 body edit (no endpoint change) | 0 | 1 | pubs=1 |
| T4 add an endpoint | 1 (`[1/2] Re-running CMake...` after the census edge ran) | 2 | pubs=2, seen by that configure |
| T5 no-op after | 0 | 2 | pubs=2 |

No "multiple rules generate", no cycle error.

Zephyr 3.7, `west build -b native_sim/native/64` on a bare hello-world app
with the same snippet (`native_sim` 32-bit needs gcc-multilib, absent on this
host; the tree uses `native_sim/native/64`):

| test | result |
| --- | --- |
| T1 clean | 0 re-runs, 1 configure |
| T2 no-op | 0 |
| T3 body edit | 0 re-runs |
| T4 add endpoint | exactly 1 re-run; the configure in that pass saw pubs=2 |
| T5 no-op after | 0 |

No errors.

**Decision: RFC-0120 D7's build-time fallback is NOT taken.** Because the
re-run is a real host configure, Kconfig is re-rendered too, so Kconfig-backed
derived values update through the same path. The Kconfig-symbol listing below
is therefore moot, and W6 stays the census-step wiring.

### W1 — `nros configure`, with sync folded in (LANDED 2026-11-03)

- A user-tier verb in the Build group (RFC-0117). It runs C1 (today's sync)
  and C2 to C6 (today's stages 1 to 4), and writes `<build>/nros/configured/`
  with a stamp holding the input digest.
- An unchanged second run is a no-op.
- `nros sync` becomes an alias: a `("sync", "configure")` row in the user-tier
  alias table (RFC-0117). With that row, `check-cli-old-spellings` (phase-526
  W4) flags every `nros sync` in the scanned tree, so run
  `python3 scripts/check/check-cli-old-spellings.py --fix` in the same change.
- `nros build` and `nros run` configure when the stamp is stale, instead of
  refusing an unsynced tree.
- The interface search path is computed inside configure. Remove
  `eval "$(nros dev ws env)"` from every book and README flow.
- The metadata probe is cached per component, keyed on the component's
  sources.
- Layout and concurrency per RFC-0120 D2:
  - stamps live at `<project>/build/nros/configured/<image>/`, plus one per
    shared host-configure coordinate;
  - a per-project file lock serialises writers;
  - the stamp records the ament prefix and the `nros-launch-resolve` path the
    run used;
  - `NROS_RESOLVED_DIR` retires in favour of locating the stamp by image.
- Measure the lock under the fixture build's parallel configures: one
  workspace reached by several configures at once.
- **Acceptance:**
  - `nros new … && nros build` succeeds with no `nros sync` typed;
  - a second `nros configure` with nothing changed exits in under one second
    (measured on the C++ workspace);
  - `nros sync` still works.

**What landed (W1).**

- **The verb.** `nros configure` is a user-tier verb in the Build group. It
  runs C1 (today's sync) and C2 to C6 (stages 1 to 4) under the project lock,
  and runs no target build tool. Flags: `--image`, `--generated-base`,
  `--ros-edition`, `--dry-run`, `--check`, `--force`, `--offline` and the
  provider and metadata flags it inherited from sync. An image the host cannot
  build is recorded as skipped and configured only when named.
- **The stamp and its staleness rules.** A stamp per image at
  `<project>/build/nros/configured/<image>/stamp.toml` holds the input digest:
  `package.xml`, `Cargo.toml`, `system.toml`, launch files, the env variables
  configure reads (`AMENT_PREFIX_PATH` among them), the `nros-launch-resolve`
  path, and the ament install's CONTENT (the digest of the value alone missed
  an in-place ROS upgrade, so a moved generated package makes the stamp stale).
  A skip stamp on a named image is stale. `--dry-run` records no stamp. A
  project with no image gets one `.project` stamp. A skip stamp re-checks its
  image's preflight. The probe cache per component and the shared host-configure
  coordinate stamps (D2) are not part of W1; see the open items below.
- **The lock.** A per-project file lock serialises writers. The parallel
  measurement the item asks for (several configures reaching one workspace at
  once, under the fixture build) is not yet run; it belongs with the fixture
  build's switch to configure.
- **Implicit configure.** `nros build` and `nros run` call
  `configure::ensure_current` before planning, like cargo fetching what it
  needs. The "has not been synced" refusal is now an internal error, since a
  configure that left the tree unsynced is a bug. `refresh_generated` stays in
  build as the ament backstop; `refresh_managed_msg_deps` is gone, covered by
  configure's C1.
- **The sweep.** `nros sync` is an alias row `("sync", "configure")`, warns on
  stderr on every use whether or not it is a terminal (W9 step 1). The tree was
  swept with `check-cli-old-spellings --fix`, so the scanned tree holds zero
  `nros sync`, and the gate keeps it so. Remedy strings in the CLI say
  `nros configure`. `eval "$(nros dev ws env)"` is gone from the book flows.
- **`--offline`.** It reaches the one step of a sync that resolves crates, the
  `cargo update --workspace` that prunes a seeded leaf `Cargo.lock`, so an
  offline configure never touches the network.

**Acceptance (measured 2026-11-03).**

- `nros new … && nros build` needs no `nros sync` typed: the build integration
  test `build_on_a_fresh_copy_needs_no_sync` passes.
- `nros sync` still works, with its warning.
- A second `nros configure` with nothing changed on `examples/workspaces/cpp`:
  **0.14 s** wall ("up to date (18 image(s))", measured twice), under the 1 s
  target. The first `nros configure` there took 66.0 s. On
  `examples/workspaces/rust` the second run is **0.06 s** (three runs), after a
  `--force` configure of 7.2 s (an earlier first run, before any stamp, took
  41.1 s). The first measurement of the C++ workspace found a self-deadlock:
  configure held the project lock while its census spawned
  `nros build demo_bringup:linux`, whose implicit configure blocked on that same
  lock (`locks_lock_inode_wait`); Ruling R20 fixed it, as configure now exports
  `NROS_CONFIGURE_HELD` to its children and an implicit configure of the same
  project returns at once (W2 removes the spawn itself).

**Not in the W1 change set (still open):** the per-component metadata probe cache,
the per-coordinate stamps for shared host configures, retiring
`NROS_RESOLVED_DIR`, and the parallel-lock measurement. They carry forward
to the later items that consume them.

### W2 — Census by default (RFC-0120 D4) (LANDED 2026-11-03)

This flips an existing opt-in to the default. The hook already takes the
census for `entities = "census"` (issue 2086), and that spelling keeps working.

- **C/C++ leaves.** A leaf with no declared `[[component]] entities` gets C4
  automatically. `entities = "defaults"` is the written opt-out.
  `NROS_LEAF_CENSUS_TAKE=OFF` stays, for a driver that already took the census.
- **Rust leaves.** These are counted by the metadata probe in C1 (RFC-0120 D4).
  First measure why phase-481 W3 saw five of six Rust leaves refused, then fix
  what stands between those leaves and a composed inventory.
- **The census's own host build.**
  - It runs on crate defaults, with no seeds and no promise checks, so it takes
    one pass. Today it builds up to three times (`entity_census.rs:376-387`).
  - The workspace census stops spawning `nros build <native>`
    (`entity_census.rs:546-560`) and builds the native image through the host
    tool directly (RFC-0120 D1).
- **Preflight and refusals.** C3 checks the host toolchain: a C/C++ compiler,
  the host Rust target and the store Corrosion. It names the one missing. A
  census that cannot build is refused, naming both fixes.
- **Acceptance:**
  - `examples/zephyr/cpp/talker` is sized from its census on both roads;
  - its xrce image no longer needs the three hand-stated
    `[image.zephyr_xrce.knobs.xrce]` caps: delete them and it still boots.
    `XRCE_MAX_SERVICE_CLIENTS` derives from the sizing descriptor
    (`knobs.toml`), so this also needs the descriptor present on that road.
    The talker comment saying the client cap "has no derivation at all" is
    stale; correct it;
  - a Rust Zephyr leaf is sized from its probe on both roads;
  - a test leaf whose source calls a Zephyr API gets the two-fix refusal;
  - the workspace census reaches no `nros build`.

**What landed (W2).**

- **One rule.** `nros_cli_core::leaf_census::entity_source` says where a
  standalone leaf's entity counts come from: `declared` (a list), `census`
  (written, or by default), `defaults` (`entities = "defaults"`, parsed by
  `leaf_system` as `LeafComponent::entities_defaults`) or `unstated`. The
  census is the default for a C/C++ leaf (a `CMakeLists.txt`, no `Cargo.toml`)
  with at least one `[[component]]`, none stating `entities`, whose image is
  CROSS. A host image keeps the hosted defaults (the line `nros build` already
  draws for a workspace: a host image is the census producer), and so does a
  leaf with no component (RFC-0120 D4 amended to say so). Every reader asks the
  rule: the leaf reader (`leaf_entity_env`), the descriptor's composer, the
  Kconfig rendering, `nros build`'s leaf road. The cmake and Zephyr hooks ask
  `nros dev image entity-census source --leaf <dir>` instead of grepping
  `system.toml` for one word; `nros_leaf_census_take` decides through it, so
  the Zephyr hook's existing call is now the default path (W4 replaces it with
  a configure call). `NROS_LEAF_CENSUS_TAKE=OFF` and `entities = "census"` keep
  working. The fixture pre-pass lists every C/C++ leaf and lets `take --leaf`
  decide ("nothing to take" for the rest).
- **The census's host build is one pass.** `take --leaf` configures and builds
  once under `NROS_LEAF_CENSUS_HOST` (no declaration, census or seed read); the
  convergence loop is gone. The stub `find_package(Zephyr)` package also
  enables C after `project()` (`CMAKE_PROJECT_INCLUDE`), because a Zephyr
  application may name `LANGUAGES CXX` alone. A Zephyr C application that
  names no `nano_ros_add_executable` is found through the census program
  manifest, which is now armed for every census leaf.
- **Preflight (C3).** `preflight::check_host_toolchain` checks a host C and C++
  compiler (`CC`/`CXX` or `cc`/`c++`), CMake, Ninja, the host Rust target and
  the store's Corrosion, and names each missing one with its fix. A census leaf
  whose host build fails is refused with both fixes (`entities = [...]`, or
  `entities = "defaults"`).
- **The workspace census reaches no `nros build`** (RFC-0120 D1). `take` plans
  the host sibling in-process (`build::plan_builds`, as configure does) and runs
  its own cargo/CMake steps (`build::build_host_image`), under
  `NROS_LEAF_CENSUS_HOST`, once. The `take` VERB configures first
  (`ensure_current`); the take inside a configure or a build does not.
- **`NROS_CONFIGURE_HELD` is retired.** It existed so the census's child
  `nros build` would not wait on its parent configure's lock. Nothing a
  configure runs calls `nros build` or `nros configure` now, and no other path
  needed it (`git grep` finds no other self-spawn of `build`/`run`).
- **Rust leaves.** Measured before any change: `west-leaf-sizing` over all six
  `examples/zephyr/rust/*` leaves refused every one ("no components were
  registered"), because no probe sidecar existed: nothing had run the metadata
  probe (C1). After `nros configure` in each leaf, all six compose
  (`basis probe sidecars`, `NROS_ENTITY_INVENTORY_STATUS "derived"`). So
  phase-481 W3's refusals were a road that ran no sync, not a composer defect;
  `nros build` runs C1 implicitly since W1, and the plain-west road gets it
  when W4's hook calls configure.
- **The test leaf that cannot be counted.** `idle-wake-zephyr-mps2` includes
  `<zephyr/devicetree.h>`; its default census was refused with the two fixes,
  and it now states `entities = ["subscription:std_msgs/msg/String:/chatter",
  "timer"]`.

- **`nros build` of a multi-image Zephyr leaf named no image.** Found while
  measuring: the west handoff of `nros build zephyr_xrce` passed no
  `-DNROS_IMAGE`, so the hook rendered the talker's default (zenoh) image over
  `prj-xrce.conf`. The west road now passes it for an application holding its
  own `system.toml`, as the cmake leaf road does.

**Acceptance (measured 2026-11-03, Zephyr 4.4, `native_sim/native/64`).**

- `examples/zephyr/cpp/talker`, census-sized on both roads. The census host
  build: ONE configure, 0 `Re-running CMake`, 165 s on the loaded host
  (1 node, 1 pub, 1 timer). A plain `west build -- -DNROS_IMAGE=zephyr_xrce`
  and `nros build zephyr_xrce` derive the same knobs from it:
  `NROS_XRCE_MAX_SUBSCRIBERS=0`, `NROS_XRCE_MAX_SERVICE_SERVERS=1` (the
  transient-local cache queryable), `NROS_EXECUTOR_MAX_CBS=1`,
  `NROS_EXECUTOR_MAX_NODES=1`, `NROS_RMW_SUBSCRIBER_SLOTS=0`; the sizing
  descriptor is named to cargo on both and states `service_client_entities =
  0`, `action_client_entities = 0`, which `XRCE_MAX_SERVICE_CLIENTS` derives
  from. The three `[image.zephyr_xrce.knobs.xrce]` caps are deleted and the
  comment corrected. Both images boot against a MicroXRCEAgent: session open,
  28 and 29 `Publishing:` lines in 15 s, no error. (Each west configure still
  ran 7 passes: the fixed point, W4/W7.)
- Rust: `examples/zephyr/rust/talker` from its probe on both roads (`entity
  inventory from its probe sidecars`; `NROS_MAX_PUBLISHERS=1`,
  `NROS_MAX_SUBSCRIBERS=0`, `NROS_MAX_QUERYABLES=1`, `NROS_EXECUTOR_MAX_CBS=1`)
  with a plain `west build` and with `nros build zephyr_zenoh`.
- `idle-wake-zephyr-mps2` (includes `<zephyr/devicetree.h>`) before its
  declaration: the host build fails at `zephyr/devicetree.h: No such file`,
  and `take --leaf` refuses naming both fixes.
- The workspace census reaches no `nros build`: `nros configure --force` of
  `examples/workspaces/cpp` with its census deleted, polled every 0.5 s for
  this checkout's `nros build`/`nros run` processes: 0 seen. It took the census
  with `demo_bringup:linux` in 220 s (one plan, one host build; the host
  image's own cmake configure still settles once internally, the fixed point
  W7 deletes), and the six other cross images read it `fresh`.
- Census host builds of other cross leaves, one pass each: zephyr/c/talker
  152 s, threadx-linux/cpp/talker 71 s, rv-virt-nuttx/c/talker 114 s,
  mps2-an385-freertos/cpp/listener 149 s, rv-virt-threadx/c/talker 734 s (load);
  linux/c/talker `unstated`, nothing taken.

### W3 — Resolve gaps: the Rust west entry, and the multi-entry union

- **Rust west entry (RFC-0120 D6 case 2).** C5 writes the message-bound seed
  and makes the sizing-descriptor decision for a Rust entry.
  - First MEASURE whether derived message-bound knobs reach the Rust lane's
    cargo environment today. `zephyr/CMakeLists.txt:384-387` already adds every
    derived-rung knob through `nros_derived_knob_env` (issue 1796), so the gap
    may be closed. It remains open only if the bound knobs resolve on another
    rung, or if a Rust entry has no bound producer at all (it calls no
    `nros_find_interfaces`). If a gap is confirmed, file it with
    `just issue-new` and fix it here.
  - Also measure whether a clean generated Rust entry arms the sizing
    descriptor's deferred backstop (`NanoRosSizingDescriptor.cmake:748-765`).
- **Multi-entry (case 3).** C5 resolves once per `cmake_coordinate` as the
  union over its images (`compose_image` already takes several models).
  `nros_resolved_entity_take` accepts N models.
- **Acceptance:** for each of the two, a clean build dir shows 0
  `Re-running CMake`, and the knobs delivered are identical to today's
  converged values.

### W4 — The Zephyr hook calls `nros configure` (cases 1 and 4)

- `nros_image_kconfig_hook` calls `nros configure` for the leaf or for the
  image owning the entry (RFC-0085 D14's walk-up). It returns at once when the
  stamp is current or `NROS_LEAF_CENSUS_HOST` is set. A guest application (no
  `system.toml`) is unchanged.
- Move the issue-1699 Kconfig stamp from `zephyr/CMakeLists.txt` into the
  hook, and add `Kconfig.from-system-toml` to its digest. Add the test it has
  never had: edit `zephyr/Kconfig`, rebuild an existing build dir, and assert
  the new default lands in one pass.
- Replace each early reader's `nros_reconfigure_settle` and each late
  producer's arm with a promise check. The sites are:
  `zephyr/cmake/nros_cargo_build.cmake`, `cmake/NanoRosCodegenCore.cmake`,
  `cmake/NanoRosNodeRegister.cmake`, `cmake/NanoRosEntry.cmake` and
  `cmake/NanoRosSizingDescriptor.cmake`. Retire the pass bookkeeping in
  `cmake/NanoRosResolved.cmake` (`_nros_reconfigure_pass_props`).
- **Acceptance:**
  - a plain `west build` of `examples/zephyr/cpp/talker` and of a Rust leaf,
    and of `examples/workspaces/rust/src/zephyr_entry`, each from a clean build
    dir: 0 `Re-running CMake`, tight knobs;
  - the same three under `nros build`: identical knobs.

**What landed (W4).**

- **The hook configures.** `nros_image_kconfig_hook` runs `nros configure
  <project> --image <image>` (`_nros_image_kconfig_configure`) before any
  reader: the project is the leaf, or the colcon workspace owning a generated
  application's bringup (`<ws>/src/<bringup>`, RFC-0085 D14's walk-up); the
  image is the one the hook renders (`-DNROS_IMAGE`, else `default_images`).
  Skipped under `NROS_LEAF_CENSUS_HOST`; a guest is unchanged. It is a query
  caller and never runs `nros build`. It replaces the hook's census take:
  configure's C4 takes a Zephyr C/C++ leaf's census now. The status line
  reports the configure's wall time.
- **Located by identity (RFC-0120 D2).** Each image's stamp records the dir
  stage 3.5 resolved into (`resolved_dir`); `nros locate configured --project
  P --image I` prints it, and the hook publishes it for this configure only
  (`NROS_CONFIGURED_RESOLVED_DIR`, a GLOBAL property). `nros build` no longer
  passes `-DNROS_RESOLVED_DIR`: nothing on the west line, the identity
  (`NROS_CONFIGURED_PROJECT`/`_IMAGE`) on the cmake line. A bare id names the
  one `<bringup>:<id>` stamp. W3 is not in this base: its coordinate dirs
  (`build/<coord>/resolved/`) reach the locator once configure records them
  as the stamp's `resolved_dir`, which is the one line its merge must add.
- **Promise checks (D5).** `cmake/NanoRosPromise.cmake`: a reader that read
  configure's seed records what it consumed; the late producers are held to
  it at the END of the configure (one hop late, so a producer called once per
  package may pass through intermediate answers): equal continues; different
  FATALs naming the seed and the fragment, quoting the producer's own reason
  when it has one (the bounds input record names a table set or entity
  inventory configure did not see, the entity take names the registration).
  Every settle and arm in `nros_cargo_build.cmake`, `NanoRosCodegenCore`,
  `NanoRosNodeRegister`, `NanoRosEntry` and `NanoRosSizingDescriptor` goes
  through it, and Resolved's `_nros_reconfigure_pass_props` bookkeeping is
  gone. Seeds are copied by CONTENT once per configure (issue 1729's record
  is retired). **Left for W5/W7:** a read with no seed (a plain cmake road,
  until W5's hook) still delegates to `NanoRosReconfigure.cmake` -- inside
  the promise module, its one remaining caller -- and the Cyclone C++
  cargo-file arm (issue 1653, `nros_sizing_descriptor_west_settle_cargo_file`
  and the arm in `nros_sizing_descriptor_record_for_west`) is untouched.
- **What actually cost the talker its passes.** Measured on the base: 4
  configures, 3 re-runs, and NON-convergent -- `NanoRosNodeRegister`'s
  standalone composer reads only `nros-metadata.json`, cannot see a census,
  and overwrote west-leaf-sizing's census fragment with a refusal on every
  pass until `NROS_RECONFIGURE_MAX_PASSES`. It now stands down when
  west-leaf-sizing produced the leaf's fragment (that fragment is a seed). The
  one real lag left was the message bounds: configure now writes a Zephyr
  C/C++ leaf's bounds seed, over tables it generates with the codegen and
  config chain the host configure uses, and the entity text west-leaf-sizing
  writes.
- **Two defects found on the way.** The stamp treated a census take's raw
  `<census>.recorded.json` as a census, so a census-sized leaf was never
  current (every hook configure re-ran configure); fixed. The generated Rust
  west entry could not configure at all on this base (issue 2232, fixed on
  W3's branch by 9d79af0cde): cherry-picked so the third road could be
  measured; it drops by patch-id when W3 lands.
- **The Kconfig stamp (D6 case 4)** moved into the hook
  (`_nros_image_kconfig_stamp`), before Kconfig, and hashes
  `Kconfig.from-system-toml` too. `tests/zephyr-kconfig-stamp-tests.sh`
  (`just check zephyr-kconfig-stamp`) configures a hello-world app with a
  stand-in module calling the real function, edits a prompted default, and
  asserts ONE configure and the new value in `.config`, for each file; its
  control (no stamp) keeps the old value on Zephyr 3.7. Zephyr 4.x adds
  every parsed Kconfig source to the `.config` checksum itself
  (`merge_kconfig_checksum`), so on 4.4 the control lands the edit too and
  the test says which line it measured: case 4 is a 3.7-line defect.
- **The Rust west entry's sizing-descriptor default** is "none"
  (`NROS_IMAGE_LANGUAGE=rust` runs no entry, so "named" waited for a writer
  it never had): today's converged value. W3's decision file supersedes it.

**Acceptance (measured 2026-11-03, Zephyr 4.4, `native_sim/native/64`, clean
build dirs; a configure loop is `west build -t build.ninja`).**

| road | base (W2/W3 logs, same commit) | W4: configures, re-runs | knobs |
| --- | --- | --- | --- |
| plain `west build` cpp/talker `zephyr_xrce` | 4, 3 -- non-convergent | **1, 0** | identical to the base's converged knobs |
| plain `west build` cpp/talker `zephyr_zenoh` (full build, `zephyr.exe` linked) | -- | **1, 0** | identical to `nros build` |
| plain `west build` rust/talker | 1, 0 | **1, 0** | identical to the base and to `nros build` |
| plain `west build` workspaces/rust generated `zephyr_entry` | cannot configure (issue 2232); W3 measured 1 re-run | **1, 0** | identical to `nros build` |
| `nros build zephyr_zenoh` cpp/talker | 4, 3 | **1, 0**; the hook's configure "up to date" in **42 ms** | -- |
| `nros build zephyr_zenoh` rust/talker (full build) | 1, 0 | **1, 0**; "up to date" in 2.1 s on the loaded host | -- |
| `nros build zephyr` workspaces/rust (full build) | 1 re-run (W3) | **1, 0**; "up to date" in **34 ms** | -- |

`examples/workspaces/rust/src/zephyr_entry` no longer exists (phase-470 W5.a
generates it under `build/zephyr-zenoh/zephyr_entry/`); the plain west build
is of that generated application, with the `APPLICATION_CONFIG_DIR` and
`EXTRA_CONF_FILE` the README names. Its knobs are the resolve's
(`NROS_MAX_PUBLISHERS=1`, `NROS_MAX_SUBSCRIBERS=1`, `NROS_MAX_QUERYABLES=1`,
`NROS_EXECUTOR_MAX_CBS=2`, `NROS_ZENOH_MAX_*=1`). The talker's are its census
(`NROS_MAX_PUBLISHERS=1`, `NROS_MAX_SUBSCRIBERS=0`, `NROS_EXECUTOR_MAX_CBS=1`,
`NROS_ZENOH_MAX_LARGE_SUBSCRIBERS=0`, …). A configure loop was measured with
`west build -t build.ninja`, and the talker also with a full build.

Measured with issue 2234 (open) in the way: after a CLI rebuild the stamp
says "up to date" while the resolve refuses every probe sidecar as stale, and
the workspace entry then configures with REFUSED counts (crate defaults) --
one pass, but loose. `nros configure --force` before the measurement, as W3
did. 2234 is now on every road the hook reaches, so it wants fixing before W6.

The `/home` filesystem of this shared host filled to 0 bytes twice during the
measurement; the runs it broke were re-run and the table holds the re-runs.

**Review round 1 (2026-11-04).**

- **The walk-up is the consumer's (Ruling R31).** Only a generated
  application (it states `NROS_IMAGE_SYSTEM_DIR`) configures `<ws>` from
  `<ws>/src/<bringup>`; a standalone leaf under some `src/` directory is its
  own project. `tests/cmake-resolved-seed-tests.sh` case W.
- **Only a CURRENT stamp seeds (Ruling R33).** The cmake road's identity is
  cached, so an edit followed by `cmake --build` re-runs cmake with no
  configure in front. `nros locate configured` now reports `current` / `stale`
  (the question configure asks before it skips), and
  `nros_configured_locate` publishes no answer from a stale stamp: that
  configure composes its own, converges as the base did, and says to run
  `nros configure <project> --image <image>`. The "could not see" FATAL names
  that command too. Case S (and its current-stamp control).
- The leaf bounds seed generates for the edition the host configure resolves
  (the image's declared one, else the default), never `$ROS_DISTRO`. The
  hook's timing is whole seconds (`string(TIMESTAMP %s)`; GNU `date +%N` is
  gone), so the table's milliseconds are the original measurement's.

**Review round 2.** The leaf seed's edition is the declared one, and the
Zephyr road now builds the host for it: the module hook applies the image's
`ros_edition` (the `NROS_LEAF_ROS_EDITION` row) as `NROS_ROS_EDITION`, and a
`-D` naming another is refused by issue 2087's rule, now one function
(`nros_ros_edition_agree`) both roads call (Ruling R34). The hook marks its
configure current only when `nros locate configured` published a current
answer. `tests/cmake-resolved-seed-tests.sh` case R drives the real hook
against a stand-in CLI.

### W5 — The CMake and NuttX hooks

- `find_package(nano_ros)` and `add_subdirectory(nano-ros)` call configure
  before any knob reader.
- ~~The NuttX `apps/external` Makefile makes it a prerequisite.~~ Ruling
  R36: that road is a GUEST road (below); a NuttX image built as a nros
  project takes the hooked CMake leaf road.
- **Acceptance:** a cmake leaf, a cmake workspace and a NuttX app each build
  from a clean dir in one pass, with knobs identical to `nros build`'s.

**What landed (W5).**

- **One helper.** `cmake/NanoRosConfigureHook.cmake`'s `nros_configure_hook`
  is the only spelling of the hooked `nros configure` call: the Zephyr hook
  (`_nros_image_kconfig_configure`, keeping W4's R31 walk-up) and the three
  CMake entries call it. Once per host configure; a no-op for a guest and
  under `NROS_LEAF_CENSUS_HOST`; a query caller, never `nros build`; it marks
  the configure CURRENT only when `nros locate configured` published a
  current answer (R33). `tests/cmake-configure-hook-tests.sh`
  (`just check configure-hook`, fast line) drives it with a stand-in CLI and
  checks the reach: every hooked entry calls it, nothing else spells the
  command.
- **The CMake entries.** `find_package(nano_ros)` (the top-level leaf, before
  the entity facts and the import; image = `-DNROS_IMAGE`, else
  `default_images`), `nano_ros_workspace()` (before its import; image =
  `-DNROS_CONFIGURED_IMAGE`, `-DNROS_IMAGE`, else the `IMAGES` the generated
  root now states: one is configured and located, several are each
  configured and none is located), and a bare `add_subdirectory(<nano-ros>)`
  (the consumer's `system.toml`).
- **Two defects the hook exposed, fixed.**
  - `nros locate configured` called every WORKSPACE stamp stale ("inputs
    changed") right after configure called it up to date: configure digests
    `NROS_INTERFACE_SEARCH_PATH` after setting it to the project's interface
    root, and a reader runs no configure. The digest now spells that default
    itself (`configure_stamp::input_digest_with`). Without it R33 left every
    hooked workspace configure unseeded.
  - Stage 3.5 composed over the registered population only on the west road,
    so a cmake workspace image was seeded with the model-only answer (2 nodes)
    while its configure composed every registration the root adds (7), and
    the promise check refused the configure -- under `nros build` on the W4
    base too. `resolve_composes_registered` now names the scope per road:
    the launch closure (west), the whole workspace (cmake).
- **Deadlock.** `nros build`'s `configure_implicitly` returns, dropping the
  project lock (`flock` on an `O_CLOEXEC` fd), before planning and the
  hand-off; the hook's configure takes a free lock. The census host build
  configure runs inside sets `NROS_LEAF_CENSUS_HOST`, which the helper
  honours first. No change was needed.
- **NuttX `apps/external` is a GUEST road (Ruling R36; RFC-0120 D5/D8
  amended).** The make road builds only the integration shell's runtime
  archive, from Kconfig (`CONFIG_NROS_RMW_*`, `CONFIG_NROS_ROS_EDITION`,
  `CONFIG_NROS_*_API`) through `NanoRosImageCrate.cmake -P`. It carries no
  nros project -- no `system.toml`, no image, and the user's application is
  not one of its inputs (the per-example staging was retired, Phase
  212.M-F.12, issue 1883) -- so layer 0 has nothing to configure, and the
  archive is sized by its Kconfig knobs. A NuttX image built as a nros
  project uses the CMake leaf road, which is hooked (measured below: 1
  configure, 0 re-runs). Making a make app a project needs a project
  identity and a knob channel: [issue 2236](../issues/2236-nuttx-make-app-as-project.md).

**Acceptance (measured 2026-11-04, clean build dirs; counts are `-- Configuring
done` over the configure and the first `ninja`, plus `Re-running CMake`).**

| subject | base (W4 tip) | W5 plain cmake | knobs vs `nros build` |
| --- | --- | --- | --- |
| cmake leaf `examples/linux/c/talker`, no stamp (the hook runs a full configure) | 1, 0 (no configure in front) | **1, 0**; hook "configured" in 106 s, then "up to date" | identical (360-line dump: env, fragments, 12 config headers) |
| NuttX app `examples/qemu-armv7a-nuttx/c/talker` (cmake leaf, census-sized, `armv7a-nuttx-eabihf`) | 1, 0 | **1, 0**; ELF linked | identical (census-tight: `NROS_EXECUTOR_MAX_HANDLES 1`) |
| cmake workspace `examples/workspaces/cpp` `linux_xrce` (generated root `build/posix-xrce-linux`) | `nros build`: promise FATAL (seed 2 nodes vs 7); plain: 2, 1 | **1, 0**, seeded: "entity inventory read from the resolve phase; this configure composed nothing" | identical but one line: the plain road's codegen commands carry `NROS_INTERFACE_SEARCH_PATH` as a `cmake -E env` prefix (same value), which `nros build` passes through the environment |
| cmake workspace `linux` (7-image coordinate `build/posix-zenoh-linux`) | `nros build`: promise FATAL; plain: 2, 1 | plain: 2, 1 (the root names 7 images: each configured, none located, so unseeded -- the base's lever); `nros build`: promise FATAL "composes over 7 models, the resolve over one" | -- (W3's per-coordinate union) |

Under `nros build`, every hook's configure said "up to date" in 0-2 s.

**Still unseeded: a standalone cmake leaf.** Configure resolves no image dir
for one (the cmake leaf road has no stage 3.5), so its readers keep the
unseeded road and print issue 0991's "cmake will run once more" -- which on
this road never fires: the bounds fragment is no configure dependency there,
so the measured pass count is 1 either way, and the producer's answer is what
the build gets. Making the leaf seeded is C5 for the cmake leaf road (what W4
did for the Zephyr C/C++ leaf, `configure_zephyr_leaf`); it is what W7 needs
before the unseeded fallback in `NanoRosPromise.cmake` can be deleted.

### W6 — The census step (or W0's fallback)

- Wire the W0 result into every hooked road: the census step is a build edge
  whose digest is a host-configure dependency.
- **Acceptance:** on the talker,
  - a body-only edit → no re-configure;
  - adding a publisher → exactly one re-configure, and the new count is
    delivered.

**What landed (W6).**

- **The census digest.** Every census take writes `<census>.digest` beside
  the census (`leaf.census.json` -> `leaf.census.digest`), write-if-changed:
  the digest of what the census SAYS -- the recorder document without
  provenance, source positions (`source` objects naming an artifact or line)
  or `trace`, plus the incomplete flag. A body edit moves the binary's digest,
  never this file. `entity-census path --leaf <dir> --digest` names it.
- **The census step** (`cmake/NanoRosCensusStep.cmake`,
  `nros_census_step(CLI LEAF)`), W0's mechanism unchanged: the digest is a
  `CMAKE_CONFIGURE_DEPENDS`, and a custom target runs `entity-census take
  --leaf` with the digest as its BYPRODUCT, so ninja refreshes it before it
  decides whether `build.ninja` is stale. A fresh census returns at once; a
  moved source re-takes it (an incremental build of `build/census-host` and a
  census run). Not in `ALL` under Ninja -- the manifest check runs it once per
  invocation, which a spike measured (in `ALL`, ninja ran it twice).
  **Under any other generator (Ruling R40)** `cmake_check_build_system` runs
  before any target, so the build that re-takes a census with new entities
  would link the OLD knobs: there the step is in `ALL`, runs `take
  --fail-if-changed`, every target of the leaf's directory depends on it, and
  a moved digest FAILS that build before anything links ("entities changed;
  build again"); the next build re-configures once. **One step per leaf (Ruling
  R41)**: guard and target (`nros_census_step_<key>`) are keyed by the leaf
  directory. No-op under `NROS_LEAF_CENSUS_HOST`, so the census host build has
  none. The call is deferred to the end of the top-level directory (the
  Zephyr road reaches it inside `find_package(Zephyr)`, before `project()`;
  test case Z). A host census build that fails during a build (`take
  --build-step`) is reported as a failed build with its directory, not as a
  leaf whose sources call an RTOS API.
- **Every census reader adds it, and none depends on the census FILE.**
  `nros_record_leaf_entity_facts` (the cmake leaf road) replaces its
  dependency on the census file, which re-ran the host configure on every
  re-take; `_nros_west_leaf_sizing` adds it when the leaf was sized from its
  census, and `west-leaf-sizing`'s `input` row names the digest. Only the
  census digest edge is W6's: the configure's digested input files are W5's
  (Ruling R38).
- **The stamp records what the census said** (`Stamp.censuses`, path ->
  entities digest). The step re-takes a census during a build, so freshness
  alone would call the stamp current over knobs derived from the old
  entities; a census that now counts others is stale, and the hook's
  configure re-derives.
- Tests: three unit tests (the digest moves with the entities only; it is
  written only when it changes; a fresh census with other entities stales
  the stamp, a re-take with the same ones does not), and
  `tests/cmake-census-step-tests.sh` (`just check census-step`, fast line):
  a real CMake + Ninja build with a stand-in CLI, T1-T5, the census host,
  and the reach.

**Acceptance (measured 2026-11-04, before the rebase onto W5's 7359f43a6f;
copies of each talker under the worktree's `tmp/`; "re-runs" are
`Re-running CMake` of the target build, from its own log).**

| road | body edit (string literal) | add a publisher (`/chatter2`) | knob before -> after |
| --- | --- | --- | --- |
| plain cmake leaf, `threadx-linux/c/talker` (`threadx-linux`, census-sized) | **0** re-runs; `main.c` rebuilt in the census host and the target | **exactly 1**; the hook's `nros configure` "configured (3 s)" | `ZPICO_MAX_PUBLISHERS` 1 -> **2** (zpico-sys `shim_constants.rs`); descriptor `publisher_count` 1 -> **2** |
| Zephyr 4.4 west, `zephyr/cpp/talker` (`zephyr_zenoh`, `native_sim/native/64`) | **0** re-runs; `Talker.cpp` rebuilt, `zephyr.elf` relinked | **exactly 1**; "configured (3 s)" | `NROS_MAX_PUBLISHERS` 1 -> **2** (build.ninja), `ZPICO_MAX_PUBLISHERS` 1 -> **2** |

Both: a clean configure + build is 1 configure, 0 re-runs, and a no-op build
after the publisher edit re-runs nothing. The first build after a CLI rebuild
re-ran once on both roads -- issue 1018's tool edge, not the census (the
hook's configure said "up to date"). On a body edit the census's own host
build (`build/census-host`) re-ran cmake once, for the same CLI rebuild; it is
the census producer, not the target configure.

**Re-measured on the final tree (review round 1, 2026-11-04; plain cmake
leaf `threadx-linux/c/talker`, no CLI rebuild in between):** clean configure
+ build 1 configure, 0 re-runs; no-op 0; body edit **0** re-runs (`main.c`
rebuilt); add a publisher **exactly 1** re-run (the hook's configure
"configured (0 s)"), `ZPICO_MAX_PUBLISHERS` 1 -> **2**, `publisher_count`
1 -> **2**; no-op after 0. On the body edit the census's own host build
(`build/census-host`) re-ran its cmake once again, though no CLI moved; that
is the producer's build inside the step, not the target's configure, and was
not chased further.

**The cmake workspace road re-takes its census too (Ruling R42, which
superseded R39's read-only check; [issue 2237](../issues/archived/2237-workspace-census-retake-in-build.md),
resolved).** A workspace image's census is taken from its HOST sibling, and
`take --image` plans that image; a build edge must not re-plan. So the take
RECORDS the host sibling's build step beside the census
(`<stem>.census.host-build.json`: the handoff -- `cmake --build` or
`cargo build` -- with `NROS_LEAF_CENSUS_HOST`), and the census step on this
road is `take --model <m> --workspace <ws> --build-step`: a fresh census
writes its digest and returns; a stale one reruns the recorded build step
(incremental, a host tool) and the census run. `NanoRosEntry.cmake` calls
`nros_census_step(MODEL ... WORKSPACE ...)` and no longer depends on the
census FILE; the digest beside the model is the configure dependency. A
census with no record (one an older CLI took) refuses in the build, naming
`take --image`, once. The read-only check step is deleted: one mechanism.
Tests: cases C (Ninja: a body edit builds green with 0 re-runs, an added
entity re-runs once and is read) and C2 (Makefiles, R40).

**Workspace road, measured (2026-11-04, `examples/workspaces/cpp`
`demo_bringup:threadx`, the generated root under plain `cmake --build`, which
`nros build` configures with Unix Makefiles).** `linux_xrce` is a HOST image
and checks no census, so the cross `threadx` image was measured instead.
`nros build demo_bringup:threadx` first: the take built the host sibling
`demo_bringup:linux` and recorded its build step (`cmake --build
build/posix-zenoh-linux/cmake`, `NROS_LEAF_CENSUS_HOST=1`). Then:

- no-op: 0 re-configures, the step says `fresh`;
- body edit (`Talker.cpp` printf string): **green, 0 re-configures**; the
  step re-took the census through the recorded host build (`1 sub / 1 pub`),
  the digest did not move;
- add a publisher (`/chatter2`, plus its `system.contract.yaml` rows -- on
  this road the census is checked against the contract, so an endpoint the
  contract lacks is a `missing-in-contract` refusal by design): first
  measured as a promise FATAL, which was [issue 2238](../issues/archived/2238-metadata-probe-partial-aggregate.md)
  (an incremental metadata probe rewrote the aggregate with the one
  re-probed component; Ruling R46, fixed in W6). Re-measured after the fix:
  the build that sees the edit runs **1 configure** (the hook's `nros
  configure` "configured (51 s)": the contract edit is a configure input,
  Ruling R38, and that configure re-took the census), green, and
  `ZPICO_MAX_PUBLISHERS` **1 -> 2**. The NEXT build ran **one more**
  configure, printed by the message-bound reader: "this image's derived
  message-bound sizes changed after the readers of this pass had run, so
  cmake will run once more" -- the unseeded message-bound fixed point
  (`NanoRosReconfigure.cmake`) that W7 deletes, not the census. So on this
  road an added endpoint costs 1 census/contract-driven configure plus 1
  fixed-point pass today: a **miss of "exactly one" by the fixed point**, to
  close with W7. A no-op after that: 0.

Found and fixed on the way: the census's own host build located the HOST
sibling's configure stamp (`nros_resolved_dir`), which nothing had configured,
so every workspace census take FATALed ("has not been configured"); the
census host build is the producer and reads no seed (case H).

### W7 — Delete the fixed point

- Map the fifth producer first: the Cyclone C++ configure-time facts (issue
  1653; `NanoRosSizingDescriptor.cmake`'s `west-cargo.toml` settle and C++
  arm). Give it a pre-configure producer or a promise check.
- Seed standalone cmake leaves (C5, like `configure_zephyr_leaf`) so the
  Promise unseeded fallback can go (Ruling R37): configure resolves no image
  dir for a standalone CMake leaf, so its readers are the last unseeded ones
  (phase-530 W5 measured it).
- Then delete `cmake/NanoRosReconfigure.cmake` and every call site.
- Retire or rewrite `tests/cmake-reconfigure-tests.sh` and
  `scripts/check-knob-fixpoint.py`. The behaviour they pinned no longer
  exists; their replacement asserts 0 re-configures.
- **Acceptance:**
  - `git grep nros_reconfigure_` is empty;
  - `check-knob-delivery` is green;
  - every W3 to W5 measurement is re-run on the final tree;
  - `just ci gate` is green.

**What landed (W7).**

- **The fifth producer, mapped (issue 1653).** On the Zephyr road the module
  compiled Cyclone's C++ TUs from `west-cargo.toml`'s CONTENT inside
  `find_package(Zephyr)`, and the entry copies the descriptor there LATER in
  the same configure (`nros_sizing_descriptor_record_for_west`), so the C++
  half compiled from the previous configure's copy and the fixed point paid a
  pass to catch up. Closed by ORDER rather than a seed: the read is deferred
  to the end of the top-level configure
  (`nros_sizing_descriptor_west_apply_cyclonedds_deferred`, one hop late),
  after every entry -- where the CMake road already applies it, from the
  entity-facts flush. `nros_sizing_descriptor_west_settle_cargo_file` and the
  arm in `record_for_west` are gone. Test: `cmake-sizing-descriptor-tests.sh`
  I5, whose control (the early read) misses the copy and whose deferred read
  sees it.
- **Standalone cmake leaves are seeded (Ruling R37).** The leaf road writes
  the leaf's entity fragment BEFORE every reader, from its declaration
  (`[[component]] entities` or its census), in the same CLI call that writes
  its descriptor (`sizing-descriptor --from-leaf --entity-cmake`), with the
  composer `west-leaf-sizing` uses for a Zephyr C/C++ leaf
  (`leaf_declaration`). It is recorded as a seed, and the node-register
  composer (which reads only `nros-metadata.json` and cannot see a census)
  stands down, as on the Zephyr leaf. A leaf that declares nothing (a host
  image keeps the hosted defaults, RFC-0120 D4) gets no fragment and keeps
  the composer. This is the leaf road's own stage-3.5 equivalent rather than
  a resolve dir in `plan_single_package_cmake`: the cmake road's only
  message-bound reader is the Zephyr module, so a cmake leaf needs no bounds
  seed (next item).
- **A promise is owed to a READER (RFC-0120 D5, amended).**
  `nros_find_interfaces()` stopped registering the entity and bounds
  fragments: on Zephyr the knob resolver's loaders already read both (seeded),
  and on the CMake roads nothing reads the bounds fragment at configure time
  (the payload classes reach cargo through the sizing descriptor). The
  entity-facts consumers that hand the counts to cargo
  (`_nros_entity_budget_env`, `_nros_declared_depth_table_env`,
  `_nros_param_store_env`) register themselves instead, so a consumer that
  ran before the composer is caught rather than silently stale.
- **A lag the fixed point had hidden, fixed.** On an image no entry
  composes (no seed), the standalone composer and the entity-facts flush are
  both deferred, and the flush could run first, reading the previous
  configure's fragment; the fixed point re-ran cmake over it. `check
  probe-shared-types` measured it once the fallback went (the FATAL below).
  The flush now runs the composer first, once per configure.
- **The unseeded fallback is gone.** `cmake/NanoRosPromise.cmake` no longer
  delegates to the fixed point: an unseeded read is held to the same
  end-of-configure comparison. A reader that read nothing or a placeholder
  (the placeholders now carry `nros_promise_placeholder_line`) used its
  defaults, which can be smaller than the image needs, and a WARNING says
  so (FATAL over a derived answer, R50 below); a
  reader that read an earlier configure's derived answer which this
  configure moved FAILS naming `nros configure`, since that answer can be
  smaller. Nothing re-runs cmake on its own.
- **Deleted:** `cmake/NanoRosReconfigure.cmake`, `tests/cmake-reconfigure-tests.sh`
  and its recipe (`reconfigure-on-change`). `scripts/check-knob-fixpoint.py`
  is rewritten as `scripts/check-knob-one-pass.py` (`just check
  knob-one-pass`): a configured build dir, configured again with nothing
  changed, moves no `NROS_RESOLVED_*` -- 0 extra passes, not a bound.
  `tests/cmake-resolved-seed-tests.sh` cases A, D and S now assert 0
  re-configures (the readers used their defaults, and say so), and new case
  X asserts the FATAL. `git grep nros_reconfigure_` is empty outside the
  design history (RFC-0094/0120, this phase, archived issues and roadmaps,
  the CHANGELOG), which names the mechanism as deleted.

**Acceptance (measured 2026-11-04 on the final tree, after the CLI rebuild;
clean build dirs; "configures" counts `-- Configuring done` in the step's log,
"re-runs" `Re-running CMake`; no log printed issue 0991's "cmake will run once
more", which no code can print any more).**

| road | clean | no-op | body edit | add a publisher | no-op after | knob |
| --- | --- | --- | --- | --- | --- | --- |
| plain cmake leaf `threadx-linux/c/talker` (census, seeded by the leaf road) | 1 configure, 0 re-runs | 0 | **0** re-runs (the one configure is the census host build's own) | **exactly 1** re-run | 0 | `ZPICO_MAX_PUBLISHERS` 1 -> **2**, `publisher_count` 1 -> **2** |
| plain cmake leaf `linux/c/talker` (host image: hosted defaults, RFC-0120 D4) | 1, 0 | 0 | 0 | 0 (no census, nothing derived to move) | 0 | 8 (the hosted default) throughout |
| plain `west build` `zephyr/cpp/talker` `zephyr_zenoh` (4.4, `native_sim/native/64`) | 1, 0 | 0 | **0** re-runs | **exactly 1** re-run | 0 | `NROS_MAX_PUBLISHERS` and `ZPICO_MAX_PUBLISHERS` 1 -> **2** |
| cmake workspace `examples/workspaces/cpp` `demo_bringup:threadx` (Makefiles) | `nros build`: 2 configures (the host sibling's census build, then the image) | 0 | **0** | **exactly 1** (the hook's `nros configure` "configured (52 s)"), knob 1 -> 2; see below | 1, then 0 | `ZPICO_MAX_PUBLISHERS` 1 -> 2 -> 3 |
| `nros build zephyr`, `examples/workspaces/rust` (the generated Rust west entry) | 1, 0 | plain `west build -d`: 0 | -- | -- | -- | `NROS_MAX_PUBLISHERS=1` |

Knobs are identical to `nros build` by construction on every road: the
plain host configure and `nros build`'s are the same configure behind the
same hook, which reports `configure: up to date` when `nros build` ran it.

**Rows added in review round 1 (same method).**

| road | clean | no-op | knob |
| --- | --- | --- | --- |
| plain `west build` `zephyr/cpp/talker` `zephyr_xrce` | 1, 0 | 0 | `NROS_EXECUTOR_MAX_CBS=1` |
| plain `west build` `zephyr/rust/talker` `zephyr_zenoh` | 1, 0 | 0 | `NROS_MAX_PUBLISHERS=1`, `NROS_MAX_SUBSCRIBERS=0` |
| `nros build zephyr_zenoh` `zephyr/cpp/talker` | 2 configures (census host build, then the image), 0 re-runs | **0** configures, 0 re-runs (was 1 before issue 2248's fix; measured 2026-11-05, build `z-cpp-nrosbuild-r48`) | `NROS_EXECUTOR_MAX_CBS=1`, `NROS_MAX_PUBLISHERS=1`, `NROS_MAX_SUBSCRIBERS=0` |
| NuttX cmake leaf, via `nros build` | 2, 0 (census host + image), base 837d21e08b the same | 0 | knob dump identical to base |
| ws `examples/workspaces/cpp` `linux_xrce` | 1, 0 | 0 | -- |
| ws 7-image linux coordinate (`linux`) | 1, 0, no promise FATAL | 0 | -- |

Knob diff, base 837d21e08b vs W7, every `NROS_*`/`ZPICO_*`/`XRCE_*` value
the configure hands the compiler (cache + generated headers): identical
for `threadx-linux/c/talker` (740 lines) and the NuttX talker (168 lines);
only the hashed group-dir names differ.

**The workspace's extra configure, attributed and fixed.** The build after an
added publisher ran a second configure because the per-package
`pkg/<pkg>/nano_ros_cpp/<msg>/nros_message_bounds.{cmake,json}` are OUTPUTS
of the build-time codegen command and were also configure DEPENDENCIES:
`NanoRosMessageBounds.cmake` `include()`d the `.cmake` (CMake records every
included file in `CMAKE_MAKEFILE_DEPENDS`) and `_nros_sizing_bound_args`
added the `.json` to `CMAKE_CONFIGURE_DEPENDS`. When codegen re-ran for a
package (5 of them here: `action_client_pkg`, `bulk_listener_pkg`, ...) it
rewrote identical content with a new mtime, and the Makefile generator's
`cmake_check_build_system` re-configured. Fixed at the writer's consumer:
the fragment is read with `file(READ)` + `cmake_language(EVAL CODE)` (no
configure edge), and the json is only counted. The edge was never a
correctness edge -- the content these files carry reaches the configure
through the seeded fragment, which the promise check holds. Re-measured on
`demo_bringup:threadx`: add a publisher = **1** configure, the build after
it **0**, the no-op after that **0**; knob 1 -> 2.

**`nros build zephyr` re-ran west's configure on every call (issue 2248,
resolved).** The west handoff passed its `-D` arguments after `--` on every
run, and `west build` re-runs CMake whenever it is given cmake options. It
now passes them only when the build dir's `CMakeCache.txt` does not already
hold every value (`cmake_args_already_cached`). Measured on
`zephyr/cpp/talker` `zephyr_zenoh` (2026-11-05): first build 2 configures
(census host build, then the image), second build **0** configures and 0
re-runs, where it was 1 before the fix. `EXTRA_CONF_FILE` is the one
value the configure itself rewrites (the module hook appends the image's
fragments and its `<build>/nros/` rendering, and strips them again each
configure), so those entries are dropped before the comparison.

**Ruled in review round 1.**

- **R47: a cmake road has no bounds seed, deliberately.** Its only
  configure-time message-bound reader is the Zephyr module; on a CMake road
  the payload classes reach cargo through the sizing descriptor. The leaf
  road seeds the entity fragment only.
- **R49: a SEEDED fragment is held to its seed whatever the reader order.**
  `nros_promise_seeded` snapshots the basis and registers the check itself, so
  a composer that later moves the seeded file FAILS the configure even when
  no reader registered (`cmake-resolved-seed-tests.sh` case Y: seed ->
  composer -> deferred flush reader; agree passes, disagree is FATAL).
- **R50: an unseeded read is a WARNING, FATAL only over a DERIVED answer.**
  A reader that read nothing or a placeholder used its defaults (Kconfig and
  crate values), which CAN be smaller than the image needs; it is a CMake
  WARNING, and FATAL when the composed fragment's `_STATUS` is `derived`,
  since then a real answer existed and the reader did not get it (case Z).
  Reading an earlier configure's answer that moved stays FATAL. The roads
  that can still reach an unseeded read are listed in
  `cmake/NanoRosPromise.cmake`'s header: a guest Zephyr app with composers,
  no CLI, a stale stamp, and a multi-image cmake root. "Derived" is the
  fragment's TOP-LEVEL status (`NROS_ENTITY_INVENTORY_STATUS` /
  `NROS_MESSAGE_BOUNDS_STATUS`); a derived sub-field under a refused top level
  only WARNS (review round 2, N2).

**Ruled in review round 2.**

- **R51: a west build dir is "already configured" only with its build
  system.** A failed first configure leaves `CMakeCache.txt` holding every
  `-D` but no `build.ninja`, and `west build` handed no options then skips
  cmake and dies at ninja. `west_build_dir_already_configured` requires the
  generator's own build file (`build.ninja`, or `Makefile`; any other
  generator answers "not configured"), so the options are passed and west
  configures again.
- **R52: an unseeded FATAL names the road's own remedy.** `nros configure ...`
  only where a CLI drives the road (`nros_configured_locate` publishes
  `NROS_CONFIGURED_COMMAND` only once a CLI resolves); otherwise re-run this
  build's configure, `cmake <build-dir>` or `west build -d <build-dir> --cmake`.
- **R53: that remedy must re-run cmake on the dir the FATAL leaves.** It
  nearly always fires in a clean dir's failed first configure (a cache, no
  `build.ninja`), where a bare `west build -d <dir>` skips cmake and fails at
  ninja. Zephyr with no CLI: `west build -d <dir> --cmake`; plain cmake:
  `cmake <dir>`; CLI-driven: `nros configure ...` then `nros build ...`
  (R51 re-passes the cmake options there). `cmake-resolved-seed` case Z runs
  the printed `cmake <dir>` on that state and asserts it configures. The
  rule covers EVERY configure-time FATAL whose remedy is a re-configure, through
  one helper, `nros_reconfigure_remedy` (`cmake/NanoRosDiagnostics.cmake`): the
  promise FATALs, seeded and unseeded, the missing-SystemModel FATAL in
  `nano_ros_entry()`, the unreadable sizing descriptor, and the foreign-SDK
  checkout guard.

**Ruled in review round 3.**

- **R54: the repair configures the dir that failed.** `nros_reconfigure_remedy`
  always names `west build -d <dir> --cmake` (Zephyr) or `cmake <dir>` for
  `CMAKE_BINARY_DIR`, preceded by `nros configure ...` where the CLI drives the
  road. `nros build --workspace <p> <i>` has no build-dir flag and configures
  its own dir, so it appears only as the alternative ("or rebuild with ...,
  which uses its own build dir"); on a hooked plain `west build -d <dir>` road
  it would have left `<dir>` broken.
- **R55: a cached cause needs a PRISTINE configure.** The helper takes
  `KIND RECONFIGURE|PRISTINE`. The foreign-SDK guard is PRISTINE: the failing
  configure cached `ZEPHYR_SDK_INSTALL_DIR` (FindZephyr-sdk) and its toolchain,
  and `zephyr_get` reads the cache before the environment, so `--cmake` would
  meet the same SDK. PRISTINE prints `west build -p always -d <dir> -b
  <board> <source>` plus "the cmake options it was first configured with
  after `--`" (Zephyr; the board is `CACHED_BOARD`, the user's spelling) or
  "remove `<dir>` and configure it from scratch" (plain cmake), after the
  store / `just zephyr setup` advice. Measured (`tmp/w7-westprobe/pristine.sh`,
  Zephyr 4.4, mps2/an385 hello_world with the module, foreign SDK hard-linked
  into a fake checkout): the guard fires and caches the foreign SDK; with the
  environment fixed, `--cmake` fires it again; a bare `west build -p always -d
  <dir>` from another cwd refuses (the wipe loses the source); the printed
  command run literally from `/`, plus the first run's `-D` options, configures
  with the store SDK cached (rc 0).
- **N8:** the generated-application FATAL (`NanoRosImageAgreement.cmake`, a
  file rewritten mid-configure, which the next configure reads) uses the
  helper, kind RECONFIGURE.

Gates for this wave: `cmake-resolved-seed` (67 checks, incl. a real
`check-knob-one-pass.py` run over a dir the suite configured and a no-op
build asserting 0 `Re-running CMake`), `cmake-sizing-descriptor`,
`cmake-entity-inventory`, `cmake-message-bounds`, `configure-hook`,
`census-step`, `knob-delivery`, `knob-one-pass`.

### W8 — Vocabulary and docs (RFC-0120 D9)

- Use "configure" for `nros configure` and "host configure" for CMake's,
  west's or make's own step, consistently across RFC-0094, CLAUDE.md, AGENTS.md
  and the book.
- `book/src/reference/cli.md` gains `nros configure`.
- The integration chapters show the main path first and their native tool
  second.
- `how-integration-works.md` states RFC-0072's narrowed scope.
- **Acceptance:** `just book` builds; the probe tracks for the Zephyr and
  first-project chapters pass.

### W9 — Retire `nros sync` (RFC-0120 D2)

`nros sync` is a temporary alias, not a permanent one (maintainer decision,
2026-11-03). It retires in three steps, the way the `ZPICO_*` knob spellings
did (phase-484 W8). Each step is its own change.

1. **Alias, with a warning on every use (LANDED with W1, 2026-11-03).**
   - The warning is printed on stderr whether or not stderr is a terminal, so
     CI logs and scripts see it too. Today `note_old_spelling` prints only on
     a TTY; `sync` gets the always-on form.
   - The warning names `nros configure`.
   - The W1 sweep leaves zero `nros sync` uses in the scanned tree, and
     `check-cli-old-spellings` keeps it at zero.
   - One behaviour change rides on the alias: on a workspace, `nros sync
     --image X` (= `nros configure --image X`) now treats X as NAMED, so a
     preflight refusal or other failure for X is an error, where the old
     `nros sync` ignored `--image` on a workspace and passed (final review M3).
2. **Refusal (after one tagged release has shipped step 1).**
   - `nros sync` exits non-zero, printing `nros sync was renamed: run
     nros configure` and the same arguments spelled for configure.
   - `tests/`, the `just` recipes, the fixture scripts and the book reach no
     `sync` (the gate already holds this).
   - The release notes name the rename.
3. **Removal (after one more tagged release).**
   - The `Sync` variant, its alias row and its `old_spelling` arm are deleted.
   - `check-cli-old-spellings` keeps refusing the spelling, with a
     `retired` reason, so a reintroduced `nros sync` in a doc or script is
     caught.

- **Acceptance:**
  - step 1: a non-TTY run of `nros sync` prints the warning;
  - step 2: `nros sync` exits non-zero with the message, and every lane is
    green;
  - step 3: `nros sync` is an unknown command, and the gate refuses the
    spelling.
- Steps 2 and 3 wait on releases, so the phase can close with them still
  open, provided W9 names the release each one waits for.

## Order

1. W0 decides W6's shape, so it comes first.
2. W1 and W2 are independent of W0.
3. W3 needs W1.
4. W4 and W5 need W1 and W2.
5. W6 needs W0 and W4.
6. W7 comes last but one, and W8 last.
7. W9 step 1 lands with W1; steps 2 and 3 follow tagged releases.

## What this phase does not do

- Hookless IDE support: the Eclipse pre-build emitter, the rust-analyzer
  include, debug configurations, and the IAR/Keil drop-in (RFC-0120 D8). A
  later phase.
- Moving build hand-off into RFC-0119 runner packages (RFC-0120 open
  question 2).
- Recording the ament install for IDE-launched configures (open question 3).
  It is filed if W4 measures that it is needed.

## Acceptance for the phase

- Every road in W3 to W5 configures in one pass from a clean build dir, and
  after each W6 edit.
- A plain `west build` and `nros build` deliver identical, tight knobs for the
  same image.
- `NanoRosReconfigure.cmake` is gone.
- `nros build` needs no `nros sync` typed by hand.
