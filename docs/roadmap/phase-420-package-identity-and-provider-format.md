# Phase 420 — package identity and the provider format

**Status (2026-09-04). W1–W7 landed; W8's gate is written but not yet
registered (see W8); W2–W5, W7 and W9 open.** Implements
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
  (Closed by W6: `build_search_path` composes four sources.)

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

- [x] **W6 — the search path.** (landed 2026-09-04) `[workspace] package_paths`
      in `nros.toml` plus `NROS_PACKAGE_PATH`, nano-ros tree first, shadowing
      **reported**: `nros ws providers` prints each package's kind, its root,
      and what it hid.
      **Acceptance, met:** `a_provider_in_a_third_configured_root_is_selected_by_name`
      selects a provider from a root that is neither the nano-ros tree nor the
      workspace; `the_listing_names_the_provider_that_was_hidden` asserts the
      printed report names the loser on both rows.

      Landed as `provider_scan::build_search_path`, with
      `default_search_path` reduced to "that function with nothing configured"
      so the two-root path is not a second implementation. Four decisions the
      RFC left open, and what was chosen:

      - **The environment APPENDS to the config; it never replaces it.** colcon's
        own precedent does not settle this — `COLCON_PREFIX_PATH` has no
        configuration file competing with it, so it never had to decide. What
        colcon has that DOES decide it is `--base-paths`, which replaces
        outright, and that is a FLAG. A flag is typed per invocation and its
        blast radius is one command; an exported variable persists for a shell
        session and reaches every `nros` and every cmake configure beneath it, so
        letting it replace a committed `package_paths` would let one developer's
        shell delete a root the repository declares — the same tree building
        differently on two machines with no diff to look at. That is exactly the
        "works here, not there" failure `default_search_path`'s own doc gave as
        the reason it refused an environment variable at all. Additive-only
        answers the objection while keeping the capability: because the search
        path is ORDERED and the LATER root wins, an env entry can still raise a
        provider's precedence over a configured one — it just cannot make the
        configured root vanish. "Replace it all" keeps its verb, `--base-paths`,
        which is a flag exactly like colcon's.
      - **A missing root is REPORTED and not fatal.** Fatal is wrong: the default
        path's own workspace entry is legitimately absent, and a porter's
        `NROS_PACKAGE_PATH` may name a tree that exists on only some machines —
        making it fatal would refuse `nros sync` on this monorepo. Silent is
        wrong too, because nobody types a path they did not mean to exist. So a
        missing root keeps its index (the numbers in a stored `ProviderIndex`
        must mean the same trees everywhere), contributes nothing, prints a
        stderr warning quoting the entry AS WRITTEN, and is marked
        `MISSING — nothing scanned` in the listing. Only CONFIGURED origins warn:
        the nano-ros tree and the workspace are exempt, because a warning printed
        on every ordinary invocation is a warning nobody reads.
      - **Relative entries resolve against the WORKSPACE, not the cwd**, and `~`
        expands while `~user` does not. `nros.toml` sits at the workspace root,
        which is what D6's own `["src", …]` example is relative to; a
        cwd-relative reading would make `nros ws providers` answer differently
        depending on where it was invoked. `~user` needs a passwd lookup, and
        left literal it becomes a missing root that says so rather than resolving
        to a directory nobody named.
      - **A repeated root is one root, keeping its FIRST index.** Scanning one
        tree twice reports every provider in it as shadowing itself, and keeping
        the first occurrence means adding an entry never renumbers the roots
        before it — `root[0]` is the nano-ros tree in every invocation.

      **The Phase 212.I `nros.toml` rejection is NARROWED, not lifted.** That
      rejection exists so a pre-212 `nros.toml` — the whole system definition,
      `[system]` and `[deploy.*]` and a `[workspace] default` — cannot be
      silently ignored. D6 then spells the search path in that same file, so a
      blanket rejection would have made the RFC's own example unusable in any
      cargo workspace: write the documented key and every `nros plan` /
      `nros codegen-system` / `nros config` refuses the workspace, quoting a
      migration for a surface you never had. A file whose ONLY content is
      `[workspace] package_paths` is now accepted; a bare `[workspace]` with no
      keys is not (it declares no search path, so it is a legacy remnant), and
      neither is anything with a legacy key beside it. No pre-212 file can pass.

      **The AMBIGUOUS line states a fact and does not prescribe a rename.** A
      same-root collision has no precedence and `resolve_unique` refuses it — but
      `board` `threadx` is legitimately claimed by `nros-board-threadx-linux` and
      `nros-board-threadx-qemu-riscv64`, separated by the descriptor's
      `target_contains`, which is phase-348 W2's finding that a flat "two
      packages, one name is an error" rule would reject a shipping arrangement.
      So the report says the by-name lookup refuses and that a caller with its
      own discriminator still resolves it, and marks BOTH rows of the tie —
      singling one out would name a selection that will not happen.

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

      **The gate is written (`scripts/check-vendor-fetch-pinned.py`, 2026-09-04);
      it is not yet registered in `just/check.just`, and the conversion half is
      NOT done. Both halves of why are below, because the second is a finding
      rather than a delay.**

      *The gate.* Same reasoning as `check-submodule-pins`, not a second one:
      that gate exists because a gitlink is a full commit id and can therefore
      be interrogated, so a pin that moves backward is DETECTABLE even though
      the diff shows two indistinguishable hex strings. A fetch is that pointer
      with the enforcement removed — `GIT_TAG v0.6.1` names a ref on someone
      else's server, and if they move it, this tree switches trees with no local
      diff at all. Accepted digest forms are `URL_HASH` at SHA256/384/512 or
      SHA3-256/384/512, and `GIT_TAG` at a FULL 40- or 64-hex commit id (the same form the
      submodule gate governs, so the two tell one story about what a pin is).
      Rejected: a tag, a branch, `HEAD`, a short sha (a prefix the remote
      resolves), a `${VAR}` GIT_TAG (not statically establishable), `URL` with no
      hash, `URL_MD5`/`SHA1` (forgeable — they answer "did the bytes arrive
      intact", not "are these the bytes I pinned"), the non-option spellings
      `URL_SHA256`/`URL_SHA512` (CMake verifies nothing and says nothing), and
      SVN/HG/CVS or a bespoke `DOWNLOAD_COMMAND`, which have no digest slot.
      `_selftest()` runs on the NORMAL path (phase-395), driving the real
      classifiers over 18 synthetic cases in both directions.

      *Vacuity is a stated fact, not a pass.* Measured 2026-09-04: **0** fetch
      declarations and **0** downloading build scripts inside any of the 407
      discovered packages, so both rules print `NOTHING TO CHECK` with the
      population they searched. The tree's only fetch is **outside** every
      package — `cmake/NanoRosCorrosion.cmake:644`, `GIT_TAG
      ${_nros_corrosion_tag}`, resolved from `[tool.corrosion] upstream` (the tag
      `v0.6.1`). Scoped to D5's sentence alone the gate would have reported OK on
      an empty set while the tree's one real fetch sat a directory up, which is
      the issue-0196 shape, so it scans everything and holds out-of-package
      findings in a shrink-only BASELINE. That fetch is the FALLBACK path — the
      supported one is the sha256-verified SDK store (`nros setup --tool
      corrosion`) — and retiring the baseline entry needs `_nros_corrosion_pin()`
      to return a commit id, i.e. an edit in `cmake/` that this wave does not own.

      *No `[source.*]` row is a safe proof subject.* 14 of the 15 rows are
      `submodule = …`, and converting one moves its pin out from under
      `check-submodule-pins` — the roadmap's own W9 concern, not a proof's job.
      The 15th, `[source.rosidl]` (the only `git`-clone row, already pinned at a
      full SHA), fails on the MERITS as well as on ownership:
      - it is a **Python source tree**, so it has no CMake target and no cargo
        `links` to export through. The only channel left is a path in a cache
        variable, which is the weakest form of D5's CMake channel and closest to
        the ambient-path shape D5 exists to remove;
      - `msg_to_cyclone_idl.py` resolves it through a deliberate RUNTIME ladder
        whose rung 2 is the host's own ROS install. A configure-time fetch would
        make a ROS-full host download a rosidl it does not need, or reintroduce
        the ladder and prove nothing;
      - the conversion needs `<depend>` + a cache-variable read in
        `packages/rmw/cyclonedds/nros-rmw-cyclonedds/{package.xml,cmake/NrosRmwCycloneddsTypeSupport.cmake}`
        and the ladder rewrite in `scripts/cyclonedds/msg_to_cyclone_idl.py`.

      A NEW vendor package was considered instead and rejected: every external
      tree this build wants is already provisioned by exactly one mechanism
      (submodule, SDK store, or the Corrosion fallback), so a vendor package for
      any of them is a SECOND SPELLING until the first is deleted — and deleting
      the first is the same blocked edit. **The shape is therefore proven where
      it can be proven honestly — by the gate's negative controls — and the
      conversion folds into W9, which already owns moving trees between the two
      pinning mechanisms.**

      *Offline (RFC-0087's Consequences).* Checked 2026-09-04: **nothing in the
      tree sets `FETCHCONTENT_BASE_DIR`**, nor `FETCHCONTENT_FULLY_DISCONNECTED`
      nor any `FETCHCONTENT_SOURCE_DIR_<UC>`; the only occurrence of the name in
      the repo is RFC-0087 line 278 proposing it. So the "once per host" property
      the RFC's offline story rests on is not in place — with the default
      `<build>/_deps`, every build directory fetches independently. The scale is
      already on record: issue 0500 measured **159** build dirs in this tree each
      carrying its own resolved Corrosion (139 on 0.5.1, 20 on 0.6.1), which is
      the same per-build-dir duplication one axis over. Setting one shared cache
      dir is a prerequisite of the first vendor package, not a follow-up to it.

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
