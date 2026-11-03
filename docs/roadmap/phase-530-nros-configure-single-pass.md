# Phase 530 — `nros configure`: one resolver for every road, tight sizing, and no fixed point

**Status (2026-11-03). OPEN — W0 done (fallback not taken), W1 landed (with W9 step 1), W2 landed, W4 landed (on the W2 branch; W3 is a sibling), W5 landed (on W4; the NuttX make road is a guest road, Ruling R36).** Implements
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
