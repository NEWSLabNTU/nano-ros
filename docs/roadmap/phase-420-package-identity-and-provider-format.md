# Phase 420 — package identity and the provider format

**Status (2026-09-04). W1–W2 landed; W3–W9 open.** Implements
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

- [~] **W9 — the in-tree vendored backends adopt the same shape.** (surveyed +
      partially landed 2026-09-05) `zpico-sys` and `xrce-sys` currently vendor
      through a submodule plus `build.rs`, which is a third mechanism. Split each
      into a vendor package (fetch/build of the upstream tree) and a provider
      package (the backend), so ours and a user's differ in nothing but location.
      Largest wave; do it after W8 has proven the shape on a simpler row.
      **Acceptance:** `zenoh-pico` and Micro-XRCE-DDS are reached by package
      name; `check-submodule-pins` still governs whichever remain submodules; no
      backend keeps a bespoke vendoring path.

      **The survey moved the wave, and two of this item's premises are false.** What landed is identity for the one directory that already IS a
      vendor package; the conversion this item names is refused, with reasons.

      **S1 — `xrce-sys` has no `build.rs`, and no crate.** phase-321 W1.d deleted
      the `xrce-sys` crate (701 LoC + a 307-line build script) for having zero
      dependents, leaving a directory holding two submodules and a README that
      says so: *"this directory is a SUBMODULE HOST, not a crate … Do not re-add
      a crate here."* So "vendor through a submodule plus `build.rs`" describes
      only `zpico-sys`.

      **S2 — the bespoke path is not the submodule, it is that every vendored
      tree is compiled TWICE, once per language lane.** Measured:

      | tree | cargo lane | cmake / west lane |
      | --- | --- | --- |
      | zenoh-pico | `zpico-sys/build.rs` → `nros-zpico-build` (2,593 LoC) | `zephyr/cmake/nros_rmw_zenoh.cmake:11` — a `GLOB_RECURSE` over the same submodule |
      | micro-XRCE + micro-CDR | `nros-rmw-xrce-cffi/build.rs:160-245` | `nros-rmw-xrce/CMakeLists.txt:143-193` — a hand-copied source list |

      The XRCE pair is the sharpest case: its lockstep is asserted by a comment
      calling it "a 115.K.2 invariant", and until this wave that comment named
      `packages/rmw/xrce/xrce-sys/build.rs` — **the file phase-321 W1.d deleted.**
      The invariant has been pointed at a ghost since phase-321.

      **S3 — the mirror has already drifted, in two respects.**
      `nros-rmw-xrce-cffi/build.rs` honours `NROS_LINK_IP=0` (phase-204.7) and
      drops `udp_transport{,_posix}.c`; `nros-rmw-xrce/CMakeLists.txt` compiles
      them unconditionally, so the CMake lane cannot build a serial-only XRCE
      node. **FIXED 2026-09-05 (issue 1068, archived).** The two lists became
      one — `packages/rmw/xrce/xrce-sources.txt`, with the conditionals carried
      as named groups so a lane supplies only a boolean per token and never
      decides which files a token covers. Neither lane names a `.c` any more,
      and `check-xrce-source-manifest` asserts that plus token agreement in
      BOTH directions, because a token only one lane answers is this same
      defect one conditional over. And the vendored VERSION is restated by hand in four places, only one of
      which agrees with the tree it describes: the client gitlink is upstream **v3.0.1**
      (`bdfa2809` = "Release v3.0.1") and micro-CDR is **v2.0.2**, while
      `nros-rmw-xrce-cffi/build.rs:359-383` bakes `2.4.1` (wrong) and `2.0.2`
      (right), `nros-rmw-xrce/CMakeLists.txt:59-62` bakes
      `2.4.1` for BOTH — `PROJECT_VERSION*` is never reset between the two
      `configure_file` calls, so `MICROCDR_VERSION_STR` compiles as `"2.4.1"` in
      the CMake lane and `"2.0.2"` in the Rust one — and `[source.micro-xrce-dds-
      client] version` in `nros-sdk-index.toml` says `2.4.3-nros1`. Nothing reads
      those macros (grepped: zero uses in either vendored tree and in ours), so
      the drift is cosmetic today; it is left ALONE on purpose, because the fix
      is to derive the version from the tree that has it, not to correct four
      literals and leave five spellings.

      **S4 — converting either to a fetch is strictly worse, and for zenoh-pico
      it is impossible.** `zenoh-pico` is a PATCH LINE: `jerry73204/zenoh-pico`
      branch `nano-ros`, pinned `c5853157`. D5's worked example is `URL` +
      `URL_HASH`, and a tarball has nowhere to put patches; a `PATCH_COMMAND`
      would resurrect the `patches/` directory `.gitmodules` records as
      deliberately deleted for the qemu fork in favour of the fork's own history.
      The remaining option, `GIT_TAG <full sha>` against our own fork, satisfies
      `check-vendor-fetch-pinned` but is the identical pointer with
      `check-submodule-pins` removed — W8's own argument for converting no row.
      The eProsima pair IS unpatched upstream (no `branch =`, upstream URL, both
      on release commits), so a fetch is mechanically possible there — and still
      buys nothing: it trades a gitlink for a weaker pin, drops the trees out of
      `nros setup --source`, and leaves the duplicated build untouched, which is
      the actual defect.

      **Decision: a fork IS a vendor package whose "fetch" is a submodule.** That
      is the finished shape, not a way-station. RFC-0087 D5 should say so — it
      currently reads as though `FetchContent` were the only spelling of "fetches
      an external source tree", and a gitlink is the strongest of the three pin
      forms, not an unmigrated one. **Recommended amendment to D5, not made here
      (`docs/design/` was outside this change's ownership):** add that a vendor
      package's fetch may be a submodule, that a PATCHED upstream must be one,
      and that `check-submodule-pins` and `check-vendor-fetch-pinned` partition
      the surface rather than ranking it.

      **Landed:**

      - `packages/rmw/zenoh/zpico-sys/package.xml` — the zenoh-pico vendor
        package gets an identity. It already owned the fetch (the gitlink), the
        build (the C compile for six platforms) and the export (`links = "zpico"`
        → `DEP_ZPICO_*`, which is D5's cargo channel, live since phase-214). The
        only thing missing was that it was not a package, so nothing could name
        it. `nros_cargo`, no `<nano_ros_provides>` — a vendor package is not a
        kind (D5).
      - `nros_rmw_zenoh` gains `<depend>zpico_sys</depend>`, so the vendored tree
        is reached BY PACKAGE NAME. Inert by construction: `topo_inner` filters a
        `<depend>` naming a package outside the scanned set
        (`provider_scan.rs:438`), and `check_declared_depends` reads only the
        workspace being built, never the nano-ros tree.
      - `nros-rmw-xrce/CMakeLists.txt` — the mirror comment now names the file
        that exists, and records both measured divergences instead of asserting a
        lockstep that does not hold.

      **Not done, and why:** `xrce-sys` gets NO `package.xml`. It builds nothing —
      its two consumers each build its contents — so any `<build_type>` it
      declared would be the same false claim D2 exists to delete, and a vendor
      package nothing builds is W8's fixture. (Checked: neither eProsima tree
      ships a `package.xml` of its own, so there is no discovery-shadowing reason
      to add one either.) XRCE's vendor package cannot be created by adding a
      file; it needs the two copies of its build to become one.

      **W9 remainder, the real content:** give each vendored tree ONE build with
      ONE source list, consumed by both lanes.
      - XRCE: a vendor build under `xrce-sys/` producing `microxrcedds_client` +
        `microcdr` targets and a cargo `links` crate, with the source list and the
        version read from the vendored tree; `nros-rmw-xrce` and
        `nros-rmw-xrce-cffi` consume it instead of listing sources. This is what
        closes "no backend keeps a bespoke vendoring path", and it re-adds the
        crate phase-321 W1.d removed — legitimately, since this one has two
        dependents, which is exactly what that deletion said it lacked.
      - zenoh: fold `zephyr/cmake/nros_rmw_zenoh.cmake`'s `GLOB_RECURSE` into the
        same source list `nros-zpico-build` computes.
      Both are build-affecting and need the submodules checked out and both lanes
      built; neither is a documentation change, and neither should be attempted
      in the same commit as the identity above.

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


## Adopted issue (2026-09-04)

* **[#1054](../issues/1054-provider-scan-prunes-the-nano-ros-root.md)** —
  `provider_scan` reads `.nros-ignore` on the root it was handed, so scanning the
  nano-ros tree finds nothing. The marker's own header (issue 0621) says it
  prunes a tree from any walk that starts ABOVE it; honouring it at the root
  inverts that. Provider discovery is this phase's subject, and a scan that
  returns an empty set silently is the worst shape it can take.
