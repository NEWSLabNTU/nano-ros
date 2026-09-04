# Phase 420 — package identity and the provider format

**Status (2026-09-04). W1–W3, W5 and W7 landed; W4, W6 and W8–W9 open.** Implements
[RFC-0087](../design/0087-package-identity-and-provider-format.md). Sequenced
with [phase-421](phase-421-serialization-format-provider.md), which implements
RFC-0088 and needs **W1 of this phase only** — the rest of this phase can land
around it.

Related: RFC-0071 (provider descriptors), RFC-0062 (dependency SSoT),
phase-347 (RMW as a declared provider), phase-348 (source-time provider
discovery), phase-349 (platform family).

## Goal

Make the in-tree providers indistinguishable from a user's. Today
`packages/rmw/*` and the platform packages are found the way a user's package
would be — `provider_scan` says so explicitly — but they are *built* through a
build type that claims to be ament's, selected through a tag whose general form
does not exist, and described by descriptors that write convention out longhand.
Close those three, add the search path and selection verbs, and the provider road
is one road.

## Findings this phase acts on (measured 2026-09-04)

- Seven `<build_type>` spellings: `ament_cargo` 157, `ament_cmake` 125, `cmake`
  75, `ament_nros` 5, `nros_entry` 2, `nros_bringup` 1, `cargo` 1.
- `colcon-cargo-ros2/setup.cfg` registers 30 tasks keyed `ros.nros.<lang>.<platform>`
  and `nros_augmentation` gates on `desc.type.startswith("ros.nros.")` — **no
  `package.xml` declares that build type**, so the path cannot fire.
- `<nano_ros …/>` — 91 files (`deploy` 90, `rmw` 51, `board` 50), all consumption.
  `<nano_ros_provides …/>` — 52 tags in 21 files (board 33, rmw 11, platform 8).
  Exactly one file carries both: `nros-rmw-zenoh/package.xml`.
- Two readers have independently confused the two directions:
  `package_xml.rs`'s test message and `cmake/NanoRosPackageXml.cmake:41–46`.
- `default_search_path` returns exactly two roots, both inside the user's repo.

## Work items

- [x] **W1 — `<nano_ros_uses kind= name=/>` and one parser for three tags.** (landed 2026-09-04)
      Add the general consumption form; define `board=` / `rmw=` on the bare
      `<nano_ros …/>` tag as sugar for it; leave `deploy=` an attribute, because
      it names a `[deploy.*]` block and is not a provider kind. Implement the
      shared rule set once (must sit inside `<export>`, non-empty `kind`/`name`,
      comments stripped) and have both the Rust parser and
      `NanoRosPackageXml.cmake` consume that one implementation.
      **Acceptance, met:** a package selecting `kind="serdes"` resolves in both
      readers with no new attribute in either; the two reader-confusion tests
      still pass; `the_sugar_and_the_general_form_resolve_identically` asserts
      the equivalence in Rust and `check-package-xml-uses` asserts it in cmake.
      **This is what phase-421 W4 needs.**

      Landed as: one `read_announcement` helper serving both announcement tags
      in `package_xml.rs` (the two readers that confused the directions each
      implemented the rule separately — one match arm cannot disagree with
      itself); `PackageXml::{uses, deploy}` with `uses_of_kind()`;
      `NANO_ROS_EXPORT_USES_<KIND>` plus `NANO_ROS_EXPORT_USES_KINDS` in
      `NanoRosPackageXml.cmake`, fed by both spellings; and a buildless
      `cmake -P` gate. `deploy=` stays an attribute in both, and a
      `<nano_ros_uses kind="deploy" …/>` is not invented to carry it.

- [x] **W2 — `nros_cmake` / `nros_cargo` build types.** (landed 2026-09-04) Teach the reader both old
      and new, mapping `ament_cargo|ament_nros → nros_cargo` with a deprecation
      warning that names the file. Add `check-build-type-spelling`: the allowed
      set, plus RFC-0087 D2's class boundary — a provider, board or entry may not
      declare `ament_*`; an interface package may not declare `nros_*`.
      **Acceptance, met** (as a ratchet — the tree is not migrated until W3):
      the gate fails on a package that crosses the boundary in either direction,
      each rule watched failing on a constructed input.

      **The survey contradicted this item's premise: NOTHING read
      `<build_type>`.** Not `package_xml.rs`, not `NanoRosPackageXml.cmake`, not
      `nros-cli-core` — which only WRITES it. So "teach the readers both
      spellings" was really "give the readers a reader", and this wave adds one
      to cmake and one to `nros-cli-core`. It sharpens the Motivation's defect 2:
      colcon keys on a build type nothing declares, and no other consumer reads
      the field either.

      Three decisions worth carrying forward:

      - `ament_nros` maps to **`nros_cmake`, not `nros_cargo`** as this doc
        first said. All five in-tree uses are cmake-side — two carry a
        `CMakeLists.txt`, three are bringups that generate a CMake root.
      - **Only the three RETIRED spellings warn.** A deprecation on
        `ament_cargo` would fire on 148 in-tree packages and on every legitimate
        interface package, training people to ignore it before W3 can act.
        Whether `ament_cargo` is wrong depends on the package's CLASS, which is
        the gate's question, not a string's.
      - The gate grew a fourth rule, `owned-declares-nothing`: 34 owned packages
        (including all 21 providers) declare no `<build_type>` at all, and
        `catkin_pkg` then reports `catkin` — the same false ament-family claim,
        made by omission.

      Classification is from evidence, and `nros_generate_interfaces` is
      deliberately NOT ownership evidence: a message package calls it, and
      counting it would classify every user interface package as firmware. The
      gate cross-checks the two build-type tables against each other (S0) rather
      than being a third copy of the vocabulary.

      **Ordering hazard for W3/W4:** the scaffolder emitters
      (`emit_package_xml.rs`, `new_system.rs`, `scaffold.rs`) must not move to
      `nros_*` before W4 re-keys `colcon-cargo-ros2`, or a freshly scaffolded
      package becomes unbuildable by colcon. `ament_nros` is safe to move
      whenever — no colcon extension ever registered it.

- [x] **W3 — rewrite the nano-ros-owned packages.** (landed 2026-09-04) Entries,
      boards, RMW / platform providers, bringups. `packages/interfaces/*` and user
      message packages are **untouched** — they are ROS 2 packages. `ament_nros`,
      `nros_entry` and `nros_bringup` fold into the pair and declare no role.
      **Acceptance, met:** 371 `package.xml` rewritten (202 `nros_cmake`, 168
      `nros_cargo`, one held — below); the baseline shrank **301 → 1**;
      `check-build-type-spelling`, `check-package-xml-comments`,
      `check-package-xml-uses` and `check-provider-announcements` green.

      **The value follows the package's BUILD SYSTEM, not its old spelling** —
      the mapping table in `build_type.rs` / `NanoRosPackageXml.cmake`
      canonicalises a legacy *string*; it does not decide a *package*. Order of
      evidence used, per package: a bringup's declared images → the driver
      `plan::driver_for` picks; a descriptor-only provider → the path its
      contribution actually reaches an image by; otherwise `CMakeLists.txt`
      before `Cargo.toml`, because a package carrying both is CMake-rooted with
      the crate imported through corrosion.

      Consequences worth recording, because each inverts the obvious guess:

      - **19 Rust packages became `nros_cmake`** — the twelve Zephyr Rust
        leaves and six ThreadX ones, plus `mixed/src/rust_heartbeat_pkg`. Each
        carries a `CMakeLists.txt`; `west`/`cmake` is the build root and the
        crate is a staticlib it imports. Cargo is downstream of CMake there,
        not the entry point.
      - **7 bringups became `nros_cargo`**, including six that said
        `ament_cmake`. A bringup owns no build file at all; it generates a
        cargo root OR a cmake root per image, and an all-Rust, non-Zephyr
        workspace only ever takes the cargo one. Where a bringup's images span
        both drivers (`workspaces/rust`, `workspaces/realtime-rust`,
        `workspaces/mixed`, `workspaces/cpp`, …) **cmake wins**, for the reason
        `driver_for` gives: corrosion makes cargo consumable from cmake and
        nothing makes cmake consumable from cargo.
      - **One correction to W2's `ament_nros` note.** W2 recorded that all five
        `ament_nros` uses were cmake-side, "three are bringups that generate a
        CMake root". Measured per package, `multi_pkg_workspace_nuttx/src/demo_bringup`
        is not: its workspace is all-Rust, `driver_for("nuttx", false)` is
        `Driver::Cargo`, and the NuttX `apps/external` shim's `context::` rule
        shells `NROS_CARGO_BUILD`. It is `nros_cargo`. The four others are
        `nros_cmake` as W2 said. The *table's* `ament_nros → nros_cmake` row is
        unaffected — it canonicalises a retired string for a reader, and the
        sweep answers a different question.
      - **The three `ambiguous` packages W2 left to W3**, each decided on which
        half a consumer can actually use:
        `examples/native/c/custom-msg` → **`nros_cmake`** (application wins: it
        calls `nano_ros_add_executable`, and its messages come from
        `nano_ros_generate_interfaces`, not `rosidl_generate_interfaces`, so no
        ROS 2 node can consume them — the interface half is local to the
        example); `examples/native/rust/custom-msg` → **`nros_cargo`** (same
        shape, cargo-rooted); `nros-tests/bins/ros-edition-pose-pub` →
        **`nros_cargo`** (a `geometry_msgs` publisher fixture with no `msg/`,
        `srv/` or `action/` dir at all — its
        `<member_of_group>rosidl_interface_packages</member_of_group>` claims
        an interface it does not ship, and should be deleted separately).
      - **73 packages that said plain `cmake` / `cargo` were swept too.** D2's
        standalone-example exception does not reach them: they are workspace
        members that `find_package(nano_ros REQUIRED)` and call
        `nano_ros_auto_add_library` / `nros_components_register_node`. Plain
        `cmake` is the same false claim as `ament_cmake`, only quieter — stock
        colcon *does* register a `ros.cmake` task, so it attempts the build and
        fails at `find_package`, which is exactly the attempt-instead-of-refusal
        D2 exists to end.

      **Two standalone port templates keep plain `cmake`, deliberately** —
      `examples/templates/cpp-port-minimal-publisher` and
      `examples/templates/rclcpp-compat-smoke`. Both are verbatim stock ROS 2
      `ament_cmake_auto` packages whose whole subject is the size of the delta;
      making them declare a nano-ros build type would edit the thing being
      demonstrated. This is D2's standalone arm, used as written.

      **The one baseline row that remains is W4's, not a residue of judgement.**
      `examples/templates/local-msg-package/src/rust_consumer` is discovered by
      the `colcon build examples/templates/local-msg-package/src/` CI job and by
      `just colcon-parity`. On the CI runner (stock colcon, no
      `colcon-cargo-ros2`) it is already skipped with a warning, so nothing
      there changes either way; on a developer host *with* the extension it is
      built today by `ros.ament_cargo`, and rewriting it before W4 re-keys the
      entry points to `ros.nros_cargo` would silently stop building it. It moves
      in W4's commit, and the baseline empties there.

      **Deliberately not moved:** the scaffolder emitters
      (`emit_package_xml.rs`, `new_system.rs`, `scaffold.rs`), for the same
      reason — a freshly scaffolded package must stay colcon-buildable until W4.

- [x] **W4 — re-key the colcon extension.** (landed 2026-09-04) Entry points
      became `ros.nros_cargo` / `ros.nros_cmake`; the 30
      `ros.nros.<lang>.<platform>` keys and the `startswith("ros.nros.")` gate
      are gone. **Acceptance, met**, measured with the host's own colcon 0.20.1
      on a two-package workspace: `colcon list` reports `(ros.nros_cargo)` /
      `(ros.nros_cmake)`; with the extension registered `colcon build` selects
      `NrosBuildTask` for both, and a full cargo-path build installs the binary,
      the `package.xml` and the ament index marker, with `colcon test` running
      it through `NrosTestTask`; with the extension absent the same workspace
      reports `No task extension to 'build' a 'ros.nros_cargo' package` and
      installs nothing.

      **The 30 keys became 2, and the two facts they used to carry moved to
      where they live.** Deleting the `ros.nros.<lang>.<platform>` string
      deleted the only place `lang` and `platform` were ever written down, so
      each had to be re-sourced (`colcon_nano_ros/manifest.py`, one reader for
      both):

      - **build system ← `<build_type>`**, which is that field's whole job. It
        is strictly better than the retired `lang` token: W3 measured 19 *Rust*
        packages that are `nros_cmake`, and `lang == "rust"` would have routed
        every Zephyr and ThreadX leaf to `cargo build` instead of west/cmake.
      - **platform ← `<export><nano_ros deploy=…/></export>`**, the RFC-0087 D3
        consumption tag, with absent meaning the host — the identical rule
        `_nros_deploy_to_platform` already applies in
        `cmake/NanoRosPackageXml.cmake`, not a new default invented here.
      - **language is not recoverable, and was not faked.** `nros_cmake` says
        CMake; nothing in any manifest says C versus C++. It turned out nothing
        needed it: `_build_cmake` never read its `lang` argument, and the
        augmentation's `_needs_c` / `_needs_cpp` were written and never read
        (C/C++ bindings are CMake's `nano_ros_generate_interfaces()` job). The
        one load-bearing question — does this workspace need Rust bindings —
        is answered from evidence instead: a `nros_cargo` package always does,
        and a `nros_cmake` package does when it carries a `Cargo.toml`.

      **A silent wrong answer became a refusal.** `PLATFORM_TARGETS.get()`
      returns `None` for an unmapped platform, and `None` is also the spelling
      for *native*, so a package deploying somewhere the task cannot
      cross-compile to would have built a host binary and reported success.
      The cargo path now names the value and the known set and returns 1.

      **The scaffolder was the one real producer of the dead spelling.**
      `cargo-nano-ros/src/scaffold.rs` emitted `nros.<lang>.<platform>` for
      Rust — so the 30 keys were reachable in principle by a scaffolded
      package, just never by a tracked one. Moving it to `nros_cargo` forced a
      second change: the `<nano_ros deploy= rmw=/>` tuple is now emitted for
      **every** language, not only C/C++, because the platform no longer rides
      in the build type and a `--platform freertos` Rust package that declared
      no `deploy` would build a host binary. `scaffold_component_rust` also
      gained an `<export><build_type>` — it emitted none at all, which
      `catkin_pkg` reports as `catkin`, the `owned-declares-nothing` claim by
      omission that W2 named, and its C and C++ siblings already declared one.

      **W3's three held-back items moved here, in this wave's commit:**
      `examples/templates/local-msg-package/src/rust_consumer/package.xml`
      (`ament_cargo` → `nros_cargo`), which empties
      `scripts/build-type-spelling-baseline.json` to `{}` — the gate now reads
      407 `package.xml` with **zero** grandfathered rows; the scaffolder
      emitters `emit_package_xml.rs` (component `nros_cargo`, bringup
      `nros_cmake`), `new_system.rs` (`ament_nros` → `nros_cmake`) and
      `scaffold.rs`; and this doc's status line. `just colcon-parity` still
      passes (3 packages finished, `install/lib/consumer/consumer` produced):
      its assertion is on the `ament_cmake` C++ `consumer`, and `rust_consumer`
      is skipped by a stock colcon exactly as it already was on the CI runner,
      which installs no `colcon-cargo-ros2`.

      **`scripts/docs/migrate-example-cmake-ament.py` was updated, not
      deleted.** Its `package.xml` half wrote `<build_type>cmake` →
      `ament_cmake`, which W3 made backwards; it now writes `nros_cmake`. The
      CMake shape it emits is unchanged — `find_package(nano_ros)` +
      `ament_package()` is still RFC-0048's ament *shape*, and only the
      ownership claim moved. Deleting it was the other option, and lost:
      `packages/testing/nros-tests/tests/example_shape.rs` still names it as
      the remedy for a leaf carrying a superseded CMakeLists, so it has to keep
      working. A dry run over the 27 native leaves reports 27 already-ament, 0
      migrated — a no-op, as it should be.

      **Not done: the drift issue.** This item asked for one filed first, so
      the history records the path was dead rather than merely renamed.
      `just issue-new` reserves an id by pushing a ref to origin, which this
      change's session could not do; the record lives here and in
      `colcon_nano_ros/manifest.py` instead.

- [x] **W5 — descriptor derivation.** (landed 2026-09-04) Derive names (from
      the announcement), cargo feature, cmake value, C define token, cffi
      feature and crate. `check-derived-descriptor-fields`: a stated derivable
      field must equal its derived value — a ratchet, so history's spellings are
      grandfathered and new drift is refused. New descriptors state only
      non-derivable facts; an absent descriptor means every default applies.
      **Acceptance, met:** the five derivable fields were deleted from ALL FOUR
      rmw descriptors (20 lines) and the generated `rmw_table.rs` is
      byte-identical — `rmw_cmake_dispatch_is_current` still passes against the
      committed `cmake/NanoRosRmwDispatch.cmake`, which is the same claim one
      lowering further down.

      **`crate` is the one field that resisted, and the RFC's table is wrong
      about it.** D4 says it derives from "the package's `Cargo.toml`". Measured
      across the four backends, that is right for one and a half:
      `nros-rmw-zenoh` names its own crate; `nros-rmw-xrce` ships **no
      `Cargo.toml` at all** and its `[rmw.provides.cargo].crate` names a SIBLING
      package (`nros-rmw-xrce-cffi`, the cffi shim beside it);
      `nros-rmw-cyclonedds` states `sys_crate = "cyclonedds-sys"`, also not its
      own; `nros-rmw-uorb` is C++ and states neither. A rule that holds for two
      of four is a convention with exceptions, which is not a convention — so
      `crate` stays authored, and the xrce row is the ratchet's one
      grandfathered entry rather than a fact explained away. (`cpp_define` never
      entered the derived set; its own comment says why.)

      **`names` is the ONLY field the announcement can source, and only for a
      single-entry package.** Boards are the counterexample:
      `nros-board-nuttx-qemu` declares two `[[board]]` entries and announces
      seven names in one flat list, and `<nano_ros_provides>` carries no
      boundary that could say which four are the ARM variant's. So **rmw went
      nameless and board did not**, and `board_crate` — which every board that
      states one states correctly — is deletable only once the reader in
      `nros-cli-core/orchestration/board_descriptor.rs` derives it. That is a
      one-crate change this wave did not own; the gate covers the field
      meanwhile, so a wrong `board_crate` fails now even though a redundant one
      is still tolerated.

      **`check-provider-announcements` grew A2n rather than a second
      mechanism.** phase-421 W4's `(glob, extract)` row shape already carried
      `extract=None`; W5 gives it a meaning that binds: a nameless family's
      descriptor must declare NO names anywhere, searched over the whole
      document rather than one known table. Deleting A2's comparison without
      that would have left `names` re-addable with no reader and no gate —
      worse than a disagreement, because it can be edited with no symptom.

      **The derivation has one implementation and three readers, cross-checked
      rather than trusted.** `cargo-nano-ros/src/derived_descriptor.rs` is
      compiled into both the library and `build.rs` (`#[path]`), so the
      generator cannot be a second spelling. Its cheap announcement scanner —
      a build script may not gain a dependency without moving `Cargo.lock` — is
      asserted equal to `package_xml::PackageXml` (quick-xml) on every in-tree
      backend by `descriptor_names_come_from_the_package_xml_reader`. The gate's
      Python copy is a checker, never a producer. That is the shape CLAUDE.md
      demands after the rmw parity map and the vtable sat green and disagreed
      by 25 symbols.

      **Two gates outside the wave's file list had to move with the schema**,
      recorded here because a gate left behind is how a schema change becomes a
      red main: `check-rmw-descriptors` required the four derivable fields to be
      PRESENT (S1) and read `names` out of the descriptor (S2/S3), so it failed
      on all four descriptors the moment they shrank. S1 now requires
      `cpp_define` alone, S2 claims names from the announcement, and S3 is
      retired — "the canonical name is the first one" became structural when
      `build.rs` started taking the first announcement.

- [ ] **W6 — the search path.** `[workspace] package_paths` in `nros.toml` plus
      `NROS_PACKAGE_PATH`, nano-ros tree first, shadowing **reported**:
      `nros ws packages` prints each package's kind, its root, and what it hid.
      **Acceptance:** a provider in an out-of-repo root is selected by name, and
      a same-named provider in two roots produces a printed shadowing report
      rather than a silent winner.

- [x] **W7 — selection verbs.** (landed 2026-09-04) `nros build
      --packages-select` / `--packages-up-to`, colcon semantics, over the
      existing topological order.
      **Acceptance, met:** `up_to_narrows_the_generated_cargo_root_to_the_closure`
      and `a_selection_narrows_the_generated_cmake_root` assert the closure and
      the "nothing else" on both root-emitting drivers.

      Landed as one pure function, `builder::discover::select(&Discovered,
      &Selection)`, applied in `plan_builds` as stage 1b. **It filters the order
      stage 1 already returned and adds no second sort** — a subset of a
      topological order, taken in place, is a topological order of the subset,
      and `a_selection_keeps_the_topological_order_it_was_given` asserts the
      filter does not disturb it. `topological_order`'s output expressed the
      filter with nothing missing.

      Three decisions, two of them divergences from colcon, both toward failing
      instead of continuing:

      - **An unmatched name is an ERROR, not colcon's warning.** colcon can be
        agnostic because the name might legitimately live in an install prefix.
        D8 says nano-ros has none — the selection resolves against the source
        tree and nothing else — so an unmatched name is a typo or a stale
        script, and warning past it narrows the build to something nobody asked
        for and then reports success. `plan::resolve` already answers an unknown
        IMAGE this way, with the available names in the message; this is the
        same answer one noun over.
      - **An incomplete selection is REFUSED.** This is the wave's headline
        question and colcon's answer does not port. `--packages-select A` where
        A needs B is colcon's way to rebuild one package against an existing
        install; here there is no install, one merged root, and per-target
        static objects — B would simply be absent from the generated
        `[workspace] members` / `add_subdirectory` set, and the failure would
        surface a layer down as an unresolved path dependency or a missing CMake
        target, which is an error about the wrong thing. So the `<depend>`
        closure of the FINAL set is checked, the hole is named, and
        `--packages-up-to <the same names>` is offered as the fix. The check
        runs over the final set rather than over `--packages-select` alone
        because an up-to closure is complete by construction and intersecting it
        is not (`an_intersection_that_punches_a_hole_is_still_refused`).
      - **The two flags compose as their INTERSECTION**, which is colcon's own
        composition — each is an independent deselecting filter — and the only
        one under which adding a flag can never widen a build. A disjoint pair
        is an error naming the composition rather than an empty build.

      Two seams worth recording, because each is a place the obvious placement
      is wrong:

      - **Images are collected BEFORE the selection is applied.** An image is
        declared by a bringup's `system.toml` and is a property of the
        workspace, exactly as the generated cargo root's member list is
        (phase-383 W9.b's reasoning). Narrowing first makes
        `--packages-select talker_pkg` answer "this workspace declares no
        `[image.*]`".
      - **`check_declared_depends` keeps the UNNARROWED set.** It walks the
        whole tree itself for `<depend>` declarations, so handing it the
        narrowed name set would report every deliberately-dropped package as an
        unresolved dependency.

      Known blind spot, stated rather than papered over: the closure check sees
      the `<depend>` graph, not a cargo `path` dependency between two crates
      that declare nothing in `package.xml`. A `cargo_only` member carries no
      `depends` by design ("cargo resolves its own dependency order from
      `Cargo.toml`"), so dropping one is still loud — the noise just comes from
      cargo rather than from here.

      One consequence for a cargo workspace: a narrowed selection narrows the
      generated root's member list, which changes `Cargo.lock`. Where `--locked`
      is injected (the repo's `scripts/bin/cargo` shim) that is a hard error
      rather than a silent re-resolve, which is the behaviour issue 0359 wants.
      Generated entry packages are build output of the images being built, not
      discovered packages, so they are not selectable and are not narrowed.

- [ ] **W8 — vendor packages, proven by one.** `check-vendor-fetch-pinned`:
      every `FetchContent_Declare` / `ExternalProject_Add` in a discovered
      package carries `URL_HASH`, and any downloading build script verifies a
      digest. Then convert **one** `nros-sdk-index.toml` `[source.*]` row into a
      vendor package as proof, choosing a row whose pin is a lag rather than a
      decision — never cyclonedds or zenoh-pico, whose pins are decisions
      (RFC-0075, issue 0507).
      **Acceptance:** the converted dependency is reached by `<depend>` on the
      vendor package's name, its values arrive through CMake targets or
      `DEP_<LINKS>_<KEY>` rather than ambient environment, and the old
      `[source.*]` row is deleted in the same commit.

- [ ] **W9 — the in-tree vendored backends adopt the same shape.** `zpico-sys`
      and `xrce-sys` currently vendor through a submodule plus `build.rs`, which
      is a third mechanism. Split each into a vendor package (fetch/build of the
      upstream tree) and a provider package (the backend), so ours and a user's
      differ in nothing but location. Largest wave; do it after W8 has proven the
      shape on a simpler row.
      **Acceptance:** `zenoh-pico` and Micro-XRCE-DDS are reached by package
      name; `check-submodule-pins` still governs whichever remain submodules; no
      backend keeps a bespoke vendoring path.

## Risks

- **W3 touched 371 files** (the estimate said ~170; the gap is the 73 plain
  `cmake`/`cargo` workspace members and the 34 packages that declared nothing).
  Mechanical, but it is exactly the kind of sweep that hides one semantic
  change. Kept the rewrite and any behavioural change in separate commits.
- **W9 moves pins.** The cyclonedds and zenoh-pico pins are decisions, not lags;
  the wave must not become an excuse to bump them.
- **W4 changes what a stock colcon does with our packages.** That is the intent,
  but it will look like a regression to anyone who was relying on the accident.

## Out of scope

Install prefixes and a sourced `setup.sh`; per-package isolated builds for
in-tree packages; a Python plugin ABI; rosdep. RFC-0087 D8 records why for each.
