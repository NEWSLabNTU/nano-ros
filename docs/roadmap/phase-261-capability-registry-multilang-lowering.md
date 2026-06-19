# Phase 261 — capability registry: multi-language lowering + `features = [...]`

Status: **W1–W4 done (2026-06-18); W5 in progress (2026-06-19)** ·
Implements
[issue 0076 §B-W4](../issues/0076-followups-config-ssot-and-safety-e2e-arc.md) (spun
out of [phase-259](archived/phase-259-safety-e2e-tails.md), where it was scoped) ·
RFC-0031 §Generalization.

> **Goal.** Make the `Capability` registry the single, language-neutral extension
> point for declared capability axes: adding one `Capability{}` row lowers the axis
> to Rust cargo features AND the C/C++ `#define`/CMake token — no per-axis plumbing
> across SystemToml / plan / generate / bake. Expose a generic
> `features = [...]` declaration in `system.toml` as the thin user surface.

## Why (the gap)

`system.toml` is the **language-neutral SSoT** (read by both the Rust planner and
the C/C++ bake), but the registry it lowers through is **Rust-specific today**:

- `Capability` (cargo-nano-ros/src/capability_resolver.rs) carries only cargo-
  feature slots: `nros_feature`, `backend_feature`, `backends_supporting`. The doc
  literally reserves `c_define` / `cmake_token` for "a future wave."
- The C/C++ `#define` lowering is **hardcoded per-axis** in
  `render_system_config_h` (codegen_system.rs):
  `if sys.safety → #define NROS_SYSTEM_SAFETY_E2E`,
  `if sys.param_services → #define NROS_SYSTEM_PARAM_SERVICES`.
- Each new axis costs ~5 edit sites: a typed `SystemToml` field, a `plan` field +
  populate, `generate.rs` feature wiring, a hardcoded bake `#define`, the registry
  row. The C/C++ path is hand-wired every time — so the abstraction is incomplete
  and a naive Rust-only `features=[...]` would not cover C/C++.

## Work items

### W1 — extend `Capability` with the reserved language slots — DONE (2026-06-18)
Added `c_define: Option<&'static str>` + `cmake_token: Option<&'static str>` to the
struct; populated the existing rows (`safety` → `NROS_SYSTEM_SAFETY_E2E` /
`NANO_ROS_SAFETY_E2E`; `param_services` → `NROS_SYSTEM_PARAM_SERVICES`, no cmake
token). Pure data, no behaviour change. Tests lock the slots + assert every
`c_define` is `NROS_SYSTEM_`-prefixed (so the W2 loop stays byte-identical).

### W2 — registry-drive the C/C++ bake — DONE (2026-06-18)
Replaced the hardcoded `if sys.safety` / `if sys.param_services` branches in
`render_system_config_h` with a loop over `capability_resolver::CAPABILITIES`
(declaration order) emitting each enabled axis's `c_define`. Added
`SystemToml::capability_enabled(declared)` mapping the registry's language-neutral
axis name onto the typed `[block] enabled` field. Byte-identical for today's two
axes — regression-locked by the existing `render_system_config_h` tests
(`NROS_SYSTEM_SAFETY_E2E` / `NROS_SYSTEM_PARAM_SERVICES` emit-when-enabled /
absent-when-disabled). A new C/C++ axis now costs one `Capability{}` row + the
typed enabled-field, no edit in the bake.

### W3 — registry-drive the Rust feature lowering — DONE (2026-06-18)
Audited `generate.rs`: the capability feature *strings* already all come from the
registry (`capability(x).nros_feature` / `.backend_feature`) — no hardcoded
`"safety-e2e"` / `"param-services"` literals in the production path (only in tests).
Generalized `board_capability_features` to loop `CAPABILITIES` (push each enabled
axis's `backend_feature` when the board advertises it) instead of a hardcoded
`safety` branch; added `NrosPlan::capability_enabled(declared)` (mirrors
`SystemToml`'s). Byte-identical today (safety is the only backend-feature axis).
**Left per-axis by design:** the entry-umbrella emission in
`generated_default_features` (a registry loop would flip feature order vs the bake —
bake emits safety-first, the entry param_services-first — breaking byte-identity; and
`param_services` has a dual trigger `param_persistence || param_services`) and
`backend_features(…, safety: bool)` (the bool is threaded from callers). These fully
generalize in W4 alongside the `features = [...]` surface.

### W4 — the `features = [...]` surface — DONE (2026-06-18)
Added `[system].features: Vec<String>` (on `SystemHeader`). Each entry is a declared
axis name (`capability(name)`); an unknown name is a hard error via
`validate_and_warn_capabilities` (typo guard, run by BOTH the bake and the planner).
`SystemToml::capability_enabled` + `NrosPlan::capability_enabled` now treat the
features list and the typed block equivalently, so `features = ["safety"]` ≡
`[safety] enabled = true` lowers byte-identically on both languages (C/C++ `#define`
via the W2 loop; the planner emits `plan.safety {crc:true-default}` / `param_services`
from the unified enabled-check). **DECISION (locked W4): deprecate the typed blocks.**
`validate_and_warn_capabilities` warns on any `[safety]` / `[param_services]` block,
steering authors to `features = [...]`; the typed blocks still work (removal is a
later wave). Tests: `features=[...]` ≡ typed blocks on the bake; unknown-feature
rejected. (Existing examples using the typed blocks now emit the deprecation warn —
a later cleanup wave migrates them to `features=[...]`.)

### W5 — cmake_token threading — IN PROGRESS (2026-06-19)

Sub-waves: **W5.1** CMake map + drift test — DONE · **W5.2** root call site — DONE ·
**W5.3** bake emits `system_config.cmake` — DONE · **W5.4** worked C/C++ `safety`
fixture (+ per-platform `include()`).

**W5.3 — DONE (2026-06-19).** `codegen_system` now emits
`nros-system/system_config.cmake` next to `system_config.h`: `render_system_config_cmake`
loops the registry + `capability_enabled` (mirroring the W2 `#define` loop) and emits
`set(NANO_ROS_FEATURES "<enabled axes>" CACHE STRING "" FORCE)`. Always emitted (empty
list when no axis) so includers never break; typed block ≡ `features=[...]`. A C/C++
bringup `include()`s it before `add_subdirectory(<nano-ros>)`; the root
`nros_lower_system_features` (W5.2) then lowers it. Per-platform inclusion + the
end-to-end build proof are W5.4. Test: `system_config_cmake_emits_features`.

**W5.2 — DONE (2026-06-19).** Root `CMakeLists.txt`: added the `NANO_ROS_FEATURES`
cache var + `include(NanoRosCapabilities.cmake)` + `nros_lower_system_features(
"${NANO_ROS_FEATURES}")` in the options block, BEFORE the core-lib
`add_subdirectory(packages/core/nros-cpp)` whose `option(NANO_ROS_SAFETY_E2E OFF)`
the lowering must precede (an `option()`/`set(CACHE)` without FORCE never overrides
the pre-FORCE-set value). Empty default ⇒ no token set ⇒ byte-identical for
non-capability builds. Full-configure integration proof lands with the W5.4 fixture.

**W5.1 — DONE (2026-06-19).** Added `nros_lower_system_features(<features>)` to
`cmake/NanoRosCapabilities.cmake`: maps each declared axis to its `cmake_token`
(`safety` → `set(NANO_ROS_SAFETY_E2E ON CACHE BOOL "" FORCE)`; `param_services`
known-but-no-token; unknown ⇒ `FATAL_ERROR`, the CMake twin of
`validate_and_warn_capabilities`). Drift guard: the Rust
`cmake_capability_map_matches_registry` test asserts every registry row has a CMake
arm + its `cmake_token` is the one the arm sets, so the hand-mirror can't skew from
the SSoT. Verified the CMake parses + lowers (`cmake -P`: `safety` →
`NANO_ROS_SAFETY_E2E=ON`; unknown → fatal).

(Original deferral note retained below for context.)

### W5 (was) — cmake_token threading — DEFERRED (2026-06-19, YAGNI)
**No clean injection point exists**, so this is a new mechanism, not a one-line
thread. Findings from the W5 exploration:
- The bake emits `.h` / `.c` / `.toml` / `.json` — **no CMake**. `system_config.h`
  informs C *source*, but the `NANO_ROS_SAFETY_E2E` CMake **option** (default `OFF`
  in `packages/core/nros-cpp/CMakeLists.txt`) must be flipped at *configure* time.
- C/C++ build knobs come from scaffold-baked `set(NANO_ROS_RMW …)` (package-level,
  `scaffold.rs`), fixture `cmake_defs` (`examples/fixtures.toml`), or manual `-D`.
  None auto-flips a **system-level** capability from the declared axis. The
  `NANO_ROS_RMW` analog is per-package; capabilities live in `system.toml` (the
  bake) → architectural mismatch.

#### W5 design (explored 2026-06-19)

**The crux is ordering, not the token.** `packages/core/nros-cpp/CMakeLists.txt`
declares `option(NANO_ROS_SAFETY_E2E "…" OFF)` and reads it **at
`add_subdirectory` time**. A C/C++ example flips a build knob by `set(...)`
*before* pulling nano-ros in:

```cmake
set(NANO_ROS_RMW zenoh)                 # BEFORE — root CMake L22 default; example overrides
add_subdirectory(<nano-ros-root> nano_ros)   # nros-cpp option() evaluates HERE (root L73)
nano_ros_node_register(...)             # AFTER
nano_ros_deploy(TARGET … RMW … DOMAIN_ID …)  # AFTER — emits deploy-metadata JSON
```

So the capability knob **cannot** ride `nano_ros_deploy` (it runs after
`add_subdirectory` — too late for `option()`). It must be set in the same
pre-`add_subdirectory` slot as `NANO_ROS_RMW`. Three pieces:

**1. C/C++ declaration surface — `set(NANO_ROS_FEATURES "safety;…")` before
`add_subdirectory`.** The native-idiom projection (RFC-0004) of `system.toml`
`features = ["safety"]`, symmetric with the hand-written `set(NANO_ROS_RMW …)`.
Hand-written for single-node C/C++ apps (no `system.toml`/bake); **bake-emitted**
for multi-component systems (below).

**2. declared→`cmake_token` map — extend `cmake/NanoRosCapabilities.cmake`.** Add
`nros_lower_system_features(<features-list>)` that maps each declared axis to its
`cmake_token` and `set(<token> ON CACHE BOOL "" FORCE)`. The Rust `Capability`
registry stays the SSoT; the CMake map is a thin mirror. Precedent: that same
module already hardcodes the board-cap map (`heap → NROS_PLATFORM_HAS_MALLOC`,
direct `file(STRINGS)` read, no generator/committed fragment). **Drift guard:** a
Rust test asserts the registry's `(declared, cmake_token)` pairs equal the CMake
module's map (parse the `.cmake`), so the two can't skew. (Alternative considered:
*generate* the `.cmake` from the registry — rejected for this scale; it adds a
build-time codegen step + committed artifact against the module's established
"no generator" ethos, for a 1–2 entry map. Revisit if the map grows large.)

**3. Lowering call site — root `CMakeLists.txt`, right after the `NANO_ROS_RMW`
block (≈L22, before `add_subdirectory(packages/core/nros-cpp)` L73):**
`nros_lower_system_features("${NANO_ROS_FEATURES}")`. nros-cpp's `option()` then
observes the forced cache value.

**Multi-component systems (the bake path):** `codegen_system` emits
`nros-system/system_config.cmake` — `set(NANO_ROS_FEATURES "<enabled axes>")` (or
the tokens directly) computed by looping `CAPABILITIES` + `capability_enabled`,
mirroring the W2 `#define` loop — and the generated/scaffolded C/C++ bringup
`include()`s it before `add_subdirectory`. Single SSoT (`system.toml`) → both the
`#define` (source) and the cache option (build).

**Touch list:** `cmake/NanoRosCapabilities.cmake` (+ root `CMakeLists.txt` call),
`scaffold.rs` (bringup template `include()` + the `NANO_ROS_FEATURES` doc line),
`codegen_system.rs` (emit `system_config.cmake`), a drift-guard test, and one
worked C/C++ fixture enabling `safety` to prove the path end-to-end.

**Deferred because:** `safety` is the only `cmake_token` (zenoh-only CRC) and **0**
examples enable it — the per-axis CMake-knob plumbing cost doesn't repeat yet (the
phase's own YAGNI gate: build it when a **2nd** `cmake_token` axis or a concrete
C/C++ safety build lands). The registry slot (W1) is already populated, so adding
W5 later is purely the bake-emit + include wiring.

## Acceptance
- Adding a `Capability{}` row makes a declared axis lower to BOTH the Rust features
  AND the C/C++ `#define` (+ CMake token) with NO per-axis SystemToml/plan/generate/
  bake edits.
- `features = ["safety-e2e"]` produces byte-identical Rust + C/C++ output to the
  typed `[safety]` block.
- A worked second axis (test-only fixture row) proves the zero-plumbing path.

## Notes
Deferred from phase-259 as YAGNI while only `safety` exercises the path; pick this
up when a **2nd** concrete capability axis is on the roadmap (then the per-axis
plumbing cost repeats and the generalization pays rent). W1+W2 (registry-drive the
bake) are worth doing FIRST even before a 2nd axis — they remove the hardcoded
`#define` drift hazard for the existing axes.
