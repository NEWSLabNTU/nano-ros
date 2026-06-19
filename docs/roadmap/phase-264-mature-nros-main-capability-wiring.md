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

### W1 — decision + the feature-flow mechanism — RESOLVED (2026-06-20, exploration)
**Finding: lifecycle/params need NO board forwarding.** `lifecycle-services` and
`param-services` are features on the **`nros` umbrella** (`nros/lifecycle-services` →
`nros-node/lifecycle-services`, `nros/Cargo.toml:128/131`), which the Entry pkg
already deps directly — so **option (a)**: the Entry adds the feature to its own `nros`
dep (one line, like B1's safety), cargo unifies `nros-node` across the graph, the
service method exists, the macro emits the call unconditionally. Only **safety** needs
board forwarding (the CRC lives in the backend the board owns — already done). So W1 is
a convention, not code: document "declare `[lifecycle]`/`[param_services]` → add the
matching `nros` feature to the Entry" (until W-future `ws sync` auto-injects it).

### W2 — lifecycle in `nros::main!`
**Integration point (found 2026-06-20):** the macro's `register` closure gets a
`RuntimeCtx<'a>` (`nros-platform/src/board/runtime.rs:159`), which exposes `param` /
`remap` / `env_var` but **not** `register_lifecycle_services` (that's on
`Executor`, `nros-node`). So W2 = **(i)** add a `RuntimeCtx` forward —
`register_lifecycle_services()` + `lifecycle_state_machine_mut()` — to the underlying
`NodeDispatchRuntime`/executor (cfg `lifecycle-services`); **(ii)** `main_macro.rs`
reads `[lifecycle]` from `system.toml` (it already opens it for
`default_launch`/`[tiers]`) and, when present, emits the registration + autostart
transitions into the run closure, mirroring `generate.rs::render_lifecycle_fn` exactly
(`register_lifecycle_services()?` then `trigger_transition(Configure[/Activate])`).
Unblocks **phase-263 A3**. Test: a workspace fixture with `[lifecycle] autostart`
builds + registers the 5 services.

### W3 — log-sink init at boot
A node's `nros_info!` needs a registered sink. The board is the only layer that knows
its sink (native → stdout; embedded → its writer). Add a `BoardPrint`/`BoardEntry`
hook (or a default-sinks associated const) so the runtime calls
`nros_log::init(<board>::default_log_sinks())` once at boot — opt-out for size-bound
embedded. `nros-board-native` delegates to `nros-board-posix`; the init lands in the
posix runtime. Unblocks **phase-263 A5**. Test: a node logging in a callback produces
output under the native entry.

### W4 — parameters: runtime value-read
**Partial already exists (found 2026-06-20):** `RuntimeCtx::param(name) -> Option<&str>`
(`runtime.rs:231`) lets a node read a LAUNCH param value at `register()` time. The gap
is the typed read in a CALLBACK: add `CallbackCtx`/`TickCtx` `parameter::<T>(name) ->
Option<T>` backed by the runtime's resolved parameter store (launch/config value over
the declared default), and bind declared parameters into that store from both
`nros::main!` and `generate.rs`. Plus `[param_services]` registration (like lifecycle,
W2 mechanism). Largest item (new core API + store plumbing). Unblocks **phase-263 A2**.
Test: a node declares + reads a parameter; a launch override changes the value.

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
