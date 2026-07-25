# Phase 306 — interface-codegen correctness (renumbered from 305 — id collision with the parallel session's ament-parity phase) (resolves 0253, 0255, 0258)

**Status (2026-07-26): ALL WAVES DONE — 0253/0255/0258 resolved.** W1
per-pkg FFI crates (local-msg-package links, nm-proven symbol ownership,
fixture-gated). W2 cyclone closure (nav_msgs end-to-end; three stacked
defects incl. idlc-reserved member escapes). W3 remap routing + ~
expansion (both twins, one seam, 700+ tests). W4 ws-remap-rust wire-level
e2e PASS (two-sided proof) + coverage gates. Residuals recorded in the
archived issues: C/C++ runtime remap cell, ComponentMeta.remaps
threading, embedded remap lanes. The three issues are one cluster: the generated
interface layer (C++ FFI crates, cyclone typesupport closure, launch remap
routing) drops or duplicates things the SSoT-plus-generated architecture
says must be derived exactly once. All fixes land in the GENERATORS
(rosidl-codegen templates/emitters, `nros_find_interfaces` cmake, entry
codegen twins) so every consumer regenerates correct — never per-consumer
patching.

Friction source: the simple-autoware-safety-island ports (2026-07-24) —
each issue names its porting-notes entry.

## Waves

### W1 — per-package FFI crates (0253)

Kill the superset-archive design: each interface pkg's FFI crate contains
ONLY its own `#[no_mangle]` `nros_cpp_{publish,serialize,deserialize}_*`
fns; dependency TYPES are imported (crate deps or type-only includes)
instead of full-source `include!()` of every preceding pkg. Then ANY
combination of interface libs links — including the residual 0253 case
(two `nros_find_interfaces` calls with different topo-last pkgs). The
NO_FFI_CRATE mitigation and the one-archive routing retire.

### W2 — cyclone-ts closure honesty (0258)

Either scope the cyclone typesupport stage to the msg types actually
reachable from the consumer's used set (the resolver knows the closure),
or fix the srv→IDL lowering's cross-pkg include emit so full packages
compile. Direction picked from the survey (cheaper + sounder wins);
either way `<depend>nav_msgs</depend>` for one msg stops failing idlc on
unrelated srv files, and the workspace-shadowing workaround becomes
unnecessary.

### W3 — remap routing + `~` expansion (0255)

Project launch `<remap>` (already parsed into `NodeSpec.remaps`) into
entity creation through BOTH entry-codegen twins (macro + CLI — the
non-drift rule), and expand `~/name` against the node name/namespace.
Model-arm schema gap (if the model carries no remaps) is filed/fixed
alongside — the model is the input SSoT, so remaps must survive
`play_launch resolve`.

### W4 — verification

- W1: a two-interface-pkg consumer fixture (the 0253 shape: pkg B depends
  on pkg A, one consumer links both; plus the residual two-call shape) —
  link must succeed, symbols each defined once.
- W2: a fixture with `<depend>` on a big pkg using one msg (the nav_msgs
  shape) — cyclone-ts stage compiles.
- W3: a launch/model with `<remap>` + `~` names — runtime e2e proves the
  remapped topic is what hits the wire.
- `just check` + affected examples; resolve + archive 0253/0255/0258.

## Non-goals

- 0254 (rclcpp compat surface) — phase-236 scope.
- 0257 (executor sizing derivation) — independent, separate change.
