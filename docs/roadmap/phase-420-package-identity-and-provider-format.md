# Phase 420 — package identity and the provider format

**Status (2026-09-04). Planning.** No work item started. Implements
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

- [ ] **W1 — `<nano_ros_uses kind= name=/>` and one parser for three tags.**
      Add the general consumption form; define `board=` / `rmw=` on the bare
      `<nano_ros …/>` tag as sugar for it; leave `deploy=` an attribute, because
      it names a `[deploy.*]` block and is not a provider kind. Implement the
      shared rule set once (must sit inside `<export>`, non-empty `kind`/`name`,
      comments stripped) and have both the Rust parser and
      `NanoRosPackageXml.cmake` consume that one implementation.
      **Acceptance:** a package selecting a provider of a family that has no
      bespoke attribute builds with no change to either parser; the two
      reader-confusion tests still pass, plus a new one asserting sugar and
      general form resolve identically. **This is what phase-421 W4 needs.**

- [ ] **W2 — `nros_cmake` / `nros_cargo` build types.** Teach the reader both old
      and new, mapping `ament_cargo|ament_nros → nros_cargo` with a deprecation
      warning that names the file. Add `check-build-type-spelling`: the allowed
      set, plus RFC-0087 D2's class boundary — a provider, board or entry may not
      declare `ament_*`; an interface package may not declare `nros_*`.
      **Acceptance:** the gate fails on a package that crosses the boundary in
      either direction, and passes on the tree as it stands after W3.

- [ ] **W3 — rewrite the nano-ros-owned packages.** Mechanical: entries, boards,
      RMW / platform providers, bringups. `packages/interfaces/*` and user
      message packages are **untouched** — they are ROS 2 packages. `ament_nros`,
      `nros_entry` and `nros_bringup` fold into the pair and declare no role.
      **Acceptance:** `check-build-type-spelling` green with the old spellings
      removed from the allowed set for those classes; `just ci gate` green.

- [ ] **W4 — re-key the colcon extension.** Entry points become
      `ros.nros_cmake` / `ros.nros_cargo`; the 30 `ros.nros.<lang>.<platform>`
      keys and the `startswith("ros.nros.")` gate go. File the drift as an issue
      first so the history records that the path was dead, not merely renamed.
      **Acceptance:** `colcon build` on a workspace of nano-ros packages selects
      `NrosBuildTask`; the same workspace under a stock colcon reports unknown
      build type rather than attempting an install.

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

- **W3 touches ~170 files.** Mechanical, but it is exactly the kind of sweep that
  hides one semantic change. Keep the rewrite and any behavioural change in
  separate commits.
- **W9 moves pins.** The cyclonedds and zenoh-pico pins are decisions, not lags;
  the wave must not become an excuse to bump them.
- **W4 changes what a stock colcon does with our packages.** That is the intent,
  but it will look like a regression to anyone who was relying on the accident.

## Out of scope

Install prefixes and a sourced `setup.sh`; per-package isolated builds for
in-tree packages; a Python plugin ABI; rosdep. RFC-0087 D8 records why for each.
