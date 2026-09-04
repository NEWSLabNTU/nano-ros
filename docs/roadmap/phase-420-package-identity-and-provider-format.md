# Phase 420 — package identity and the provider format

**Status (2026-09-04). W1–W3 landed; W4–W9 open.** Implements
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

- [ ] **W4 — re-key the colcon extension.** Entry points become
      `ros.nros_cmake` / `ros.nros_cargo`; the 30 `ros.nros.<lang>.<platform>`
      keys and the `startswith("ros.nros.")` gate go. File the drift as an issue
      first so the history records that the path was dead, not merely renamed.
      **Acceptance:** `colcon build` on a workspace of nano-ros packages selects
      `NrosBuildTask`; the same workspace under a stock colcon reports unknown
      build type rather than attempting an install.

      **W3 left three things for this wave to move, in this wave's commit:**
      `examples/templates/local-msg-package/src/rust_consumer/package.xml`
      (`ament_cargo` → `nros_cargo`), which is what empties
      `scripts/build-type-spelling-baseline.json` to zero; the scaffolder
      emitters `emit_package_xml.rs` / `new_system.rs` / `scaffold.rs`; and this
      doc's status line. All three are the same hazard — a package that declares
      `nros_*` before the entry points exist is skipped by colcon with a warning,
      not refused loudly, so the regression is quiet.

- [ ] **W5 — descriptor derivation.** Derive names (from the announcement),
      cargo feature, cmake value, C define token, cffi feature and crate.
      `check-derived-descriptor-fields`: a stated derivable field must equal its
      derived value — a ratchet, so `cpp_define`'s historical spellings are
      grandfathered and new drift is refused. New descriptors state only
      non-derivable facts; an absent descriptor means every default applies.
      **Acceptance:** deleting the six derivable fields from one existing rmw
      descriptor changes no generated output.

- [ ] **W6 — the search path.** `[workspace] package_paths` in `nros.toml` plus
      `NROS_PACKAGE_PATH`, nano-ros tree first, shadowing **reported**:
      `nros ws packages` prints each package's kind, its root, and what it hid.
      **Acceptance:** a provider in an out-of-repo root is selected by name, and
      a same-named provider in two roots produces a printed shadowing report
      rather than a silent winner.

- [ ] **W7 — selection verbs.** `nros build --packages-select` /
      `--packages-up-to`, colcon semantics, over the existing topological order.
      **Acceptance:** `--packages-up-to <entry>` builds the entry and its
      dependencies and nothing else.

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
