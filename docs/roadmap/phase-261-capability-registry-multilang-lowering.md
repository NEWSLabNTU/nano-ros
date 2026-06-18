# Phase 261 — capability registry: multi-language lowering + `features = [...]`

Status: **Planned (2026-06-18)** · Implements
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

### W3 — registry-drive the Rust feature lowering
Confirm `generate.rs` (`backend_features`, `board_capability_features`, the entry
`nros/<feature>`) reads everything from the registry rows (already mostly true);
remove any residual per-axis literals.

### W4 — the `features = [...]` surface
Add a generic `features: Vec<String>` to `system.toml` / `SystemToml`. Each entry
resolves via `capability(name)`; an unknown name is a hard error (typo guard). It
lowers identically to the typed block — `features = ["safety-e2e"]` ≡
`[safety] enabled = true` — on every language. Keep the typed blocks as
sugar-over-the-same-registry (or deprecate them in a later wave; decide in W4).

### W5 — cmake_token threading (optional)
If `cmake_token` is populated, thread it into the C/C++ codegen as a
`-D<token>=ON` analog to `NANO_ROS_RMW`/`NANO_ROS_SAFETY_E2E`, so a declared axis
also flips the CMake build knob (not just the informational `#define`).

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
