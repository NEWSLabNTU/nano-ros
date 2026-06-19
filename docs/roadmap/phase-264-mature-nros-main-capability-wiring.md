# Phase 264 — mature `nros::main!` to wire the full system.toml capability set

Status: **Planned (2026-06-20)** · Resolves [issue 0089](../issues/0089-declarative-node-pkg-api-gaps-block-workspace-feature-demos.md)
· Unblocks phase-263 A2 / A3 / A5 + the showcase halves · RFC-0015, RFC-0024, RFC-0032.

> **Goal.** Make the plain-cargo workspace Entry (`nros::main!`) honour the same
> `system.toml`-declared capabilities the `codegen-system` **bake** already lowers, so
> the starter/showcase workspaces can demonstrate lifecycle, logging, and parameters —
> not just pub/sub, timer, services, tiers, and safety.

## Why (the 0089 finding, 2026-06-20)

Implementing phase-263 A1–B2 proved `nros::main!` is a **thin** macro: it emits one
`register` per launch `<node>` and **resolves `[tiers]`** (`main_macro.rs` imports
`resolve_tiers` → `run_tiers`), but does **not** wire the rest of the `system.toml`
capability set. Those are honoured only by the bake (`generate.rs`) or the `[[bin]]`
executor shape. So the workspace (cargo) build mode gates:

| Capability | Bake (`generate.rs`) | `nros::main!` today |
| --- | --- | --- |
| nodes (`register`) | ✓ | ✓ |
| scheduling tiers | ✓ | ✓ |
| `[lifecycle]` (REP-2002 services + autostart) | ✓ `render_lifecycle_fn` | ✗ |
| log-sink init | (board/runtime) | ✗ — node logs vanish |
| `[param_services]` + parameter values | ✓ | ✗ |
| safety (CRC) | ✓ (features) | ✓ *only if the entry sets the cargo features by hand* |

`generate.rs` is the reference implementation for every row; this phase ports the
missing rows into `main_macro.rs` (+ the small core/board hooks they need).

## Cross-cutting decision — the cargo-feature-flow wrinkle

The bake **writes** the generated Cargo.toml, so it adds `nros/lifecycle-services`,
`nros/param-services`, `nros/safety-e2e` as needed. `nros::main!` **cannot** edit the
(user-authored) Entry Cargo.toml, and macro-emitted `#[cfg(feature = …)]` resolves
against the ENTRY crate's features, not `nros`'s. Options (decide in W1):

- **(a) Entry opts in, macro emits unconditionally.** The Entry Cargo.toml enables the
  needed `nros` features when its `system.toml` declares the capability (as B1 does for
  safety today); `nros::main!` emits the registration unconditionally and fails loud if
  the feature is missing. Simplest; one manual line per capability in the Entry.
- **(b) Board carries forwarding features.** The board crate exposes
  `lifecycle-services` / `param-services` features (like `nros-board-native/safety-e2e`)
  that forward to `nros`; the Entry enables them on the board dep. Mirrors the safety
  precedent; keeps the Entry's `nros` dep clean.
- **(c) `nros ws sync` injects them.** Teach `ws sync` to read `system.toml` and set the
  Entry's `nros`/board features in the managed block — fully automatic, no manual line.
  Most magic, most moving parts.

Recommended: **(b)** for the runtime-service features (consistent with safety) + **(c)**
as a follow-up for zero-touch. Lock in W1.

## Work items

### W1 — decision + the feature-flow mechanism
Pick (a)/(b)/(c) above; implement the chosen feature-forwarding for
`lifecycle-services` + `param-services` (board features, per (b)). No macro emit yet —
just make the features reachable from the Entry. Verify a hand-written entry compiles
with them on.

### W2 — lifecycle in `nros::main!`
`main_macro.rs` reads `[lifecycle]` from `system.toml` (it already opens the file for
`default_launch`/`[tiers]`); when present, emit
`runtime.register_lifecycle_services()?;` + the autostart drive into the `run_plan`
closure, mirroring `generate.rs::render_lifecycle_fn`. Gate behind the W1 feature.
Unblocks **phase-263 A3**. Test: a workspace fixture with `[lifecycle] autostart`
builds + the binary registers the 5 services.

### W3 — log-sink init at boot
A node's `nros_info!` needs a registered sink. The board is the only layer that knows
its sink (native → stdout; embedded → its writer). Add a `BoardPrint`/`BoardEntry`
hook (or a default-sinks associated const) so the runtime calls
`nros_log::init(<board>::default_log_sinks())` once at boot — opt-out for size-bound
embedded. `nros-board-native` delegates to `nros-board-posix`; the init lands in the
posix runtime. Unblocks **phase-263 A5**. Test: a node logging in a callback produces
output under the native entry.

### W4 — parameters: runtime value-read
Add a parameter-value accessor to `CallbackCtx`/`TickCtx` (e.g.
`ctx.parameter::<T>(name) -> Option<T>`) backed by the runtime's resolved parameter
store (launch/config value over the declared default), and have `nros::main!` +
`generate.rs` bind the declared parameters into that store. Plus `[param_services]`
registration (like lifecycle). The largest item (new core API + store plumbing).
Unblocks **phase-263 A2**. Test: a node declares + reads a parameter; a launch override
changes the value.

## Sequencing
W1 (feature mechanism) → W2 (lifecycle — smallest macro change) → W3 (log-init —
board) → W4 (parameters — core API, largest). Each ships independently + reopens its
phase-263 wave. Keep `generate.rs` and `main_macro.rs` behaviourally aligned (a shared
helper where practical) so the bake and cargo paths can't drift.

## Acceptance
- A workspace built with `nros::main!` (not the bake) demonstrates lifecycle, logging,
  and parameters — closing phase-263 A2/A3/A5 in the cargo shape.
- The bake and `nros::main!` produce equivalent runtime behaviour for the same
  `system.toml` (no cargo-vs-bake capability gap).
- Issue 0089 resolved.

## Notes
- This is the prerequisite the phase-263 re-sequence (2026-06-20) gated A2/A3/A5 behind.
  B1 (safety) + B2 (tiers) already shipped because they needed no macro change.
- Don't fold log-init into every board blindly — gate it so size-bound embedded targets
  opt out (W3).
