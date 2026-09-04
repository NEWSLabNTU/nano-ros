# Phase 314 — one feature-set SSoT for every language

**Status (2026-09-04): COMPLETE.** W1–W5 landed 2026-07-28 and issue 0311
closed then; the last acceptance item — the Rust path hand-listing capability
features — was closed by **phase-315 W1/W2**, and the "Known deviation" below by
**phase-323 W2**. Both landed before 2026-08-01; this doc simply never recorded
it. Verified against `main` 2026-09-04, see the two entries for the evidence.
Fixes issue 0311. Unblocks multi-edition ROS support and image-level selectable
capabilities (`param-services`, `lifecycle-services`, `safety-e2e`).

## Problem

The cargo feature set for the runtime is assembled independently in three cmake
sites, five `cmake/platform/*.cmake` files, and every Rust leaf `Cargo.toml`.
Nothing checks that they agree.

| site | edition | rmw | platform | capabilities |
| --- | --- | --- | --- | --- |
| `packages/api/nros-cpp/CMakeLists.txt` | **hardcoded `ros-humble`** | inline copy | inline chain (+ `NANO_ROS_BOARD` threadx split) | param/lifecycle on posix, safety-e2e |
| `packages/api/nros-c/CMakeLists.txt` | **hardcoded `ros-humble`** | inline copy | inline chain | — |
| `cmake/NanoRosRuntimeCrate.cmake` | `ros-${_NRR_EDITION}` ✔ | `nros_rmw_dispatch()` SSoT ✔ | `_nros_runtime_platform_features()` | none |
| every Rust leaf `Cargo.toml` | `"ros-humble"` by hand | — | — | per-leaf |

Two consequences, both observed:

**A non-humble build is a wire mismatch, not a build error.** RFC-0056 makes the
edition drive the runtime keyexpr format so it matches the codegen-baked
`type_hash`. Only the umbrella honours the configured edition (phase-304 W2b);
the two direct paths compile the runtime as humble regardless. The image links,
boots, and silently fails to interoperate.

**Rust leaves make multi-edition impossible.** `nros/src/lib.rs:110` declares
`ros-{humble,iron,jazzy}` mutually exclusive via `compile_error!`, and cargo
features are ADDITIVE. A leaf saying `features = ["ros-humble"]` is not an
overridable default — it adds `ros-humble` to the unified set. An entry
selecting jazzy in a workspace of humble-naming leaves gets both and fails to
compile. Every Rust node package in the tree names its edition today.

And the trap that produced issue 0304: a consumer hook
(`NROS_EXTRA_CPP_FEATURES`) added to one assembly silently does nothing on the
others. The phase-308 probe linked a `libnros_cpp.a` with no
`nros_cpp_metadata_dump` in it, and nothing reported that the feature had gone
nowhere.

## The reframe

**The edition and the capabilities are IMAGE-level, not package-level.**

Cargo feature unification already makes this true — a workspace resolves one
feature set for one `nros` rlib, so a per-leaf edition is not a choice, it is a
constraint on everyone else. The fix is not "thread the edition through more
places"; it is to stop naming it in places that cannot own it.

That is the same shape as phase-308's conclusion for the metadata recorder: one
mechanism, several front-ends. Here the mechanism is the feature computation and
the front-ends are C, C++ and Rust.

## Waves

### W1 — settle the divergences (decisions, not code)

The three cmake assemblies do NOT agree today, so collapsing first would change
behaviour silently. Each difference is decided on its merits:

- **edition** — the direct paths must honour `NANO_ROS_ROS_EDITION` (it already
  exists and drives codegen). A defect fix, not a preference.
- **rmw** — `nros_rmw_dispatch()` is already the resolve_rmw SSoT; the inline
  copies in `nros-c` / `nros-cpp` defer to it.
- **platform** — the umbrella's `_nros_runtime_platform_features()` is WEAKER:
  it has no `NANO_ROS_BOARD` disambiguation, so `threadx` cannot split
  `threadx-linux` (std) from `riscv64-qemu` (no_std). Naive unification
  REGRESSES it. The direct chain's logic is the one to keep.
- **capabilities** — `param-services` / `lifecycle-services` / `safety-e2e`
  exist only on the C++ direct path, gated on `NANO_ROS_PLATFORM STREQUAL
  "posix"`. **Open question for whoever knows the intent:** is the umbrella's
  omission deliberate (workspace entries opt in elsewhere) or an oversight? W2
  cannot start until this is answered.

**Done when:** each row above has a decision recorded in this doc, including the
capabilities answer.

#### W1 decisions (2026-07-28) — DONE

| divergence | decision | basis |
| --- | --- | --- |
| **edition** | direct paths honour `NANO_ROS_ROS_EDITION`; no `ros-humble` literal survives | defect fix — RFC-0056 ties the edition to the keyexpr format that must match the codegen-baked `type_hash` |
| **rmw** | `nros_rmw_dispatch()` wins; the inline copies go | it is already declared the resolve_rmw SSoT |
| **platform** | keep the DIRECT chain's logic; the umbrella helper is replaced by it, not the reverse | `_nros_runtime_platform_features()` has no `NANO_ROS_BOARD` input, so it cannot split `threadx-linux` (std) from `riscv64-qemu` (no_std). Unifying onto it regresses threadx |
| **capabilities** | the umbrella's omission is a GAP, not intent — the unified function carries them | see below |

**The capabilities answer, from the code rather than from intent.**
`nros_synth_runtime_umbrella` returns early when a workspace has no Rust node
dirs (`cmake/NanoRosRuntimeCrate.cmake`: *"pure-C / pure-C++ workspace — keep
nros_cpp-static as the umbrella"*). So:

* **pure C/C++ workspace** → direct path → gets `param-services` /
  `lifecycle-services` / `safety-e2e`;
* **mixed workspace** (Rust + C/C++) → umbrella path → does NOT.

A mixed workspace declaring `[param_services]` with a C++ node would therefore
build a runtime without the feature. That is a gap, not a decision — the two
paths serve different workspace SHAPES, and a capability is a property of the
system, not of whether a Rust node happens to be present.

It is latent today only because `examples/workspaces/mixed` declares no
capabilities. That is a coverage hole, not evidence of correctness, and W5's
gate is what turns it into a caught error.

**Also found: a capability SSoT already exists**, and W4 should extend it rather
than invent one. `cargo-nano-ros`'s `capability_resolver` is the registry of
`(declared, cmake_token)` pairs; `cmake/NanoRosCapabilities.cmake` mirrors it and
a drift test (`cmake_capability_map_matches_registry`) already asserts the two
never skew. Today `safety` → `NANO_ROS_SAFETY_E2E`, while `param_services` and
`lifecycle` carry no cmake token because the direct path hardcodes them
always-on for hosted. W4 becomes: give them real tokens and let the one feature
function read them, which also removes the posix-only special case.

### W2 — one computation, several callers

A single cmake function taking `(edition, rmw, platform, board, capabilities)`
and returning the feature list. `nros-c`, `nros-cpp` and `NanoRosRuntimeCrate`
become callers. `NROS_EXTRA_CPP_FEATURES` then applies once, by construction.

`cmake/platform/*.cmake` keep their platform-specific *toolchain* knowledge but
stop carrying feature lists.

**Done when:** exactly one `set(_features …)`-style assembly exists in the tree,
and the phase-308 probe's `metadata-mode` works through it with no per-path hook.

**Done (2026-07-28)** for the three assembly sites: `nros-c`, `nros-cpp` and the
umbrella all call `nros_feature_set()`; ~180 lines of duplicated mapping gone.
Verified by building `examples/native/cpp/talker` clean.

**Found during implementation, absent from the W1 analysis:** the two crates
spell the same RMW selection DIFFERENTLY — `cffi-zenoh-cffi` / `cffi-xrce-c` in
nros-c versus `rmw-zenoh-cffi` / `rmw-xrce-cffi` in nros-cpp. A real vocabulary
difference, not an alias, so the function takes a `CRATE` argument. Renaming the
features to match would be a nicer end state and is a separate change.

**A follow-up I claimed and then disproved.** I recorded `cmake/board/*` and
`cmake/platform/*` (6 files) as still carrying their own `platform-*` feature
lists. They do not. The matches were directory paths
(`packages/platform/nros-platform-freertos`) and `NROS_PLATFORM_LINK_FEATURES`, a
TRANSPORT axis (tcp / udp_unicast / udp_multicast) unrelated to cargo features.

The three assembly sites were the only ones. The bad claim came from grepping a
feature NAME instead of an assignment, and the W5 gate inherited the same
mistake — it reported six files that were not duplication at all. Both are
corrected: the gate now matches `set(_platform_features` / `set(_rmw_features`
and covers `cmake/**` rather than being scoped to three files.

### W3 — the edition leaves the Rust leaves

Node packages stop naming `ros-*` in their `Cargo.toml`. The entry (pure-cargo)
or the synthesized umbrella (cmake/Zephyr) supplies it, and unification carries
it to every dependent.

`nros sync` should flag a leaf that still names an edition — the failure is
otherwise a `compile_error!` far from the cause.

Touches every Rust node package in `examples/` plus the templates and the
scaffold that generates new ones, so the scaffold must land in the same wave or
new packages reintroduce the problem immediately.

**Done when:** no `Cargo.toml` under `examples/` names a `ros-*` feature, and a
workspace builds with the edition selected only at the entry.

### W4 — capabilities become explicit inputs

`param-services` / `lifecycle-services` / `safety-e2e` become named arguments to
the W2 function, driven by the system's declared capabilities (`system.toml`
already carries `[param_services]`) rather than by a platform test.

**Done when:** enabling a capability in `system.toml` is what turns the feature
on, on every language path, and the posix-only special case is gone.

**Done (2026-07-28), with one deliberate deviation.** A declared capability now
flows on every platform and every language path: `NANO_ROS_FEATURES` (the cmake
projection of `[system].features`) is passed straight into `nros_feature_set`,
which maps the axis names. The registry needed no new `cmake_token` — passing
axis names rather than tokens is simpler, and the existing drift test
(`cmake_capability_map_matches_registry`) still passes.

The posix-only case is KEPT, not removed. On hosted builds `param-services` /
`lifecycle-services` stay always-on because the C++ executor headers call the
gated C entry points, so an example using them must link whether or not the
system declared the axis. It is now a superset of the declared set rather than
the only source, so nothing regresses. Removing it is a separate change that
needs every hosted example audited first.

### W5 — gate agreement

Assert the C, C++ and umbrella paths produce the same list for the same inputs,
and that no Rust leaf names an edition.

This wave is the point of the phase. Every failure this issue is about was
SILENT — a hook that did nothing, an edition that was ignored, a feature that
never reached cargo. Without a gate they drift back, and the next symptom is
again a wire mismatch or a link error a build away from the cause.

**Done when:** the gate runs in `just check` and fails on a divergence.

**Done (2026-07-28).** `scripts/check-feature-set-ssot.sh`, wired into
`just check`. Three assertions: one edition source, one platform mapping across
the converted sites, and no lib-only node package naming a `ros-*` feature.

The gate immediately earned its place — it failed on first run and found the
25 embedded node packages W3 had MISSED, because their features are written as
an inline array (`features = ["alloc", "rmw-cffi", "ros-humble"]`) and the W3
edit only matched the multi-line form. Two of its three initial failures were
its own false positives (a comment mentioning the old hardcode; standalone
examples that ARE the image and legitimately own the edition), which is worth
recording: a gate whose heuristics are wrong teaches people to ignore it.

## Non-goals

- **Changing what any feature means.** This phase moves where the list is
  computed, not what the runtime does with it.
- **Multi-edition CI lanes.** RFC-0058 / phase-309 own the test harness for
  running against several editions; this phase only removes the blocker that
  makes selecting one impossible.

## Acceptance

- [x] `nros-c` and `nros-cpp` honour the configured ROS edition; no `ros-humble`
      literal survives in cmake. *(gate-enforced)*
- [x] Exactly one feature-list computation, with the threadx board split intact.
- [x] No Rust leaf names a `ros-*` feature; the scaffold does not emit one
      (`scaffold_rust` already takes the edition as a parameter). *(gate-enforced)*
- [x] A capability declared in `system.toml` enables its feature on the C, C++
      **and Rust** paths alike. **CLOSED by phase-315 W1/W2** (verified on
      `main` 2026-09-04). `nros sync` generates a selection facade crate per
      entry, and `orchestration/facade.rs:204-223` walks
      `capability_resolver::CAPABILITIES`, testing each against
      `sys.capability_enabled(cap.declared)` — the declaration in `system.toml`
      is what puts `param-services` on the entry's `nros` dep, through cargo
      feature unification. The entries stopped restating it: the two params
      entries name the facade and no capability
      (`examples/workspaces/features/src/zephyr_rust_params_entry/Cargo.toml`,
      whose comment says so — "This entry names none of them: the declaration is
      the SSoT"). The one remaining hand-list is the NODE pkg
      (`rust_param_talker_pkg`), and it is deliberate and documented in place:
      `ctx.parameter::<T>()` is gated on the feature, so the node declares it to
      compile standalone rather than only when an Entry unifies it in. That is
      the same "reason to stay written down" resolution phase-315's last
      acceptance item uses for its three surviving paths.

      *Original wording, kept because it is the record of what was true on
      2026-07-28:* C and C++ done; Rust's silent-failure half closed, the
      derivation half not. A Rust entry still hand-listed
      `param-services` in its own `Cargo.toml`
      (`examples/workspaces/ws-params-rust/src/native_entry`), so declaring
      `[param_services]` in `system.toml` does not enable the cargo feature —
      the two are still kept in sync by hand.

      What WAS closed is the dangerous part: the mismatch used to be silent.
      `apply_param_services` is a no-op without the feature, so the build
      succeeded, the image booted, and `ros2 param list` returned nothing.
      `nros::main!` now const-asserts `__macro_support::PARAM_SERVICES_ENABLED`
      when the system declares the axis, so a mismatch fails the build naming
      the fix.

      (A `#[cfg(not(feature = "param-services"))]` in the generated code does
      NOT work: the feature belongs to `nros`, and the entry enables it through
      its dependency, so only `nros` can report it. The first attempt failed on
      a correctly-configured entry for exactly that reason.)

      Deriving the entry's feature list from the declared capabilities — so the
      two cannot disagree at all — moves to
      [phase-315](phase-315-declaration-drives-rust-selection.md), which widens
      it correctly: the capability list is only one of FOUR axes a Rust entry
      restates by hand (edition, capabilities, RMW, transport tier), and the
      derivation belongs in a generated facade crate rather than an edit to the
      user's `Cargo.toml`.
- [x] A gate in `just check` fails when the paths disagree.
- [x] Issue 0311 closes.

## Known deviation — RESOLVED by phase-323 W2

**Resolved 2026-07-31, recorded here 2026-09-04.** phase-323 W2 is titled
"delete the `posix` always-on" and did exactly that; `main` carries no
unconditional capability append, and `cmake/NanoRosRuntimeCrate.cmake:226-234`
now carries the removal's reasoning in place of the code. The audit this
deviation said was needed ahead of removal is phase-323's own W2 measurement
round.

*What the deviation said, kept as the record:* the `posix` special case was
KEPT rather than removed. On hosted builds `param-services` /
`lifecycle-services` stayed always-on, because the C++ executor headers call
the gated C entry points and an example using them must link whether or not the
system declared the axis. It was a SUPERSET of the declared set rather than the
only source, so nothing regressed — but the original acceptance wording ("the
posix-only special case is gone") was not met, and removing it needed every
hosted example audited first.
