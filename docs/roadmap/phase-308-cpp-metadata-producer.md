# Phase 308 — the C/C++ source-metadata producer

**Status (2026-07-26): W2 (the adapter) COMPLETE. W1/W3/W4 remain.** Split out of phase-307 W3 once the Rust half
landed and the C/C++ half proved phase-sized on its own. Phase-307 delivered
the producer (W1), the trigger (W2), the consumer (W4), the coverage gate (W5)
and the bake-time regression test (W6 lane 1) for **Rust**. C and C++ node
packages still have no producer, so their entity counts fall back to the
SystemModel's timer-blind lower bound — the residual half of issue 0257.

Six example node packages are affected today, named by the phase-307 W5 ledger
test (`cpp_producer_gap_is_tracked_not_hidden`). That test is the acceptance
signal: when this phase lands, its `unsupported` list goes empty and the
assertion tightens from a bound to zero.

## The layering: what is per-language and what must not be

The producer is three layers, and only the first is irreducibly per-language.
Getting this boundary wrong is the whole risk of this phase, because the thing
being produced is a COUNT, and two counters is how counts drift.

| Layer | Per-language? | Rust (landed) | C/C++ (this phase) |
| --- | --- | --- | --- |
| **Front-end** — construct the object, invoke its declaration path | Yes, irreducibly | generated harness crate calling `record_node_metadata::<Class>` | generated probe TU constructing the class and calling `configure` |
| **Adapter** — translate declaration calls into recorder calls | Small, per-language | `MetadataRecorder` as the `NodeContext` sink | recording RMW backend + two executor hooks (below) |
| **Recorder + schema + slot accounting** | **No — exactly one** | `MetadataRecorder` + `to_source_metadata_json` | **the same two**, reached from `nros-cpp` |

The third row is a hard constraint, not an aspiration, and it is nearly free:
`nros-cpp`'s implementation is itself Rust, so the recording backend and the
hooks can feed the same `MetadataRecorder` VALUE and dump through the same
`to_source_metadata_json`. Then there is one definition of what a slot is and
one schema emitter; only the adapter differs.

**Gate for it:** the C/C++ recording path must contain no serialization of its
own — no JSON, no schema struct, no slot arithmetic. If a reviewer finds any of
those three in `nros-cpp`, the layer boundary has been crossed. The consumption
side already works this way after phase-307's correction: the slot rule lives
once in `nros_orchestration_ir::sidecar_slots`, because the CLI bake and the
`nros::main!` macro both read sidecars and must agree.

## Why the adapter cannot also be shared

The tempting deeper unification is to record at the executor + session layer
for BOTH languages, so even the adapter is common. Rejected, for one concrete
reason: **the schema is declaration-flavored.** It carries `unresolved_topic:
{value, kind}` and `declaration_slot` — pre-resolution names, in declaration
order. By the time a call reaches the RMW those names are resolved and the
information is gone.

The C++ ABI still carries the user's literal declared string, which is why an
ABI-level intercept works there; the Rust equivalent of that boundary is
`NodeContext`, which is where the recorder already sits. Same conceptual layer,
different language surface. That is a legitimate per-language adapter, not a
fork.

Two things the executor-layer version WOULD buy, both currently blind spots of
*both* mechanisms — worth revisiting only if they start to matter:

- **Non-declarative registrations.** Board glue and macro-emitted timers never
  pass through `register()` / `configure()`, so nothing sees them. Handled
  today by counting them as a known constant in the bake.
- **Tier gating.** `create_entity` early-returns for entities whose callback
  group is inactive on the running tier; the recorder has no such gate, so a
  multi-tier entry's metadata OVER-counts. Safe direction, but inaccurate.

Neither is worth losing the unresolved names for.

## Why it is not a small wave

The Rust producer works because the recorder and the runtime consume the same
`Component::register` declaration path, and a host harness can call it with a
recording sink in ~40 lines of generated code. C++ has no such seam:

- A C++ node declares its entities inside `configure(nros::Node&)`, against
  real `nros::Node` methods.
- Those methods call the `nros_cpp_*` C ABI directly
  (`nros-cpp/include/nros/nros_cpp_ffi.h`, ~137 exported functions).
- So "record instead of create" has to happen at the C ABI, not above it.

Which C ABI functions, and why it is still bounded, is the next section.

## The adapter, settled (2026-07-26, after reading the seam)

Neither of the two candidates below is the right shape. Reading
`nros-cpp/src/*.rs` shows the declaration path splits cleanly in two, and the
split lands almost entirely on an extension point that already exists:

- **Publishers, subscriptions, services, service clients, actions** all reach
  `ctx.session` — i.e. they go through the RMW seam. A **recording RMW
  backend** sees every one of them, with their topic/service names and type
  names, as ordinary backend calls. That is not a fork of anything: it is a
  backend, the RFC-0054 vtable's supported extension point, selected by name.
  `Executor::open_with_rmw(rmw_name, config)` already exists, so
  `nros_cpp_init` needs no metadata-mode branch at all — it opens against the
  recording backend the same way it opens against zenoh.

- **Timers and guard conditions** reach `ctx.executor` directly
  (`register_timer_on`, `register_guard_condition`) and never touch the
  session. They are invisible to any RMW-level intercept — which is exactly
  why the "just use a recording RMW backend" shortcut was rejected earlier in
  this document, and the rejection stands as far as it goes. But the exception
  is *two functions*, not a reason to abandon the approach: hook
  `nros_cpp_timer_create*` and `nros_cpp_guard_condition_create` and the gap
  closes.

So the work is: one recording backend + two hooks + a dump export, instead of
a ~20-function recording mode (a) or a shim that forks ABI behaviour (b). The
must-not-fork constraint is satisfied structurally rather than by discipline,
and the "timers are executor-side" fact — the same fact that makes the
SystemModel blind and this whole phase necessary — is isolated to the two
places where it actually applies.

Note the pleasing symmetry with issue 0257: the model cannot see timers
because launch wiring has no timer entity, and an RMW-level recorder cannot
see them because they never reach the RMW. Same blind spot, two layers.

## Two candidate designs (superseded by the section above; kept for the reasoning)

**(a) Metadata mode inside `nros-c`.** A cargo feature under which the
declaration-path exports (`nros_cpp_node_create*`, `publisher_create`,
`subscription_create`, `timer_create` + variants, `service_{server,client}_create`,
`action_{server,client}_create`, `guard_condition_create`, plus no-op
`init`/`shutdown`/`spin`) record into the same `MetadataRecorder` the Rust
harness uses and dump the shared schema. ~20 of the 137 functions are on the
declaration path; the rest can stay unimplemented in this mode and abort loudly
if called, which doubles as a check that the probe only ever runs declarations.

Pro: one recorder, one schema, no fork — the phase-307 W3 note's explicit
requirement. Con: a second behavioural mode inside the crate that backs every
C/C++ image, so the mode gate must be airtight.

**(b) A standalone recording shim library.** A small C/C++ TU defining the same
declaration-path symbols, linked with the user's component instead of `nros-c`.

Pro: zero risk to the shipping runtime. Con: it IS a fork of the ABI's
behaviour, and drift between shim and runtime is silent in exactly the way
`check-ffi-struct-mirrors` exists to prevent elsewhere. The header being the
SSoT limits the damage (a signature change is a compile error) but semantic
drift is not caught.

**(a) is preferred** on the "must not fork" rule phase-307 W3 states, and
because phase-236 is separately turning the phase-235 recording NodeContext
into a real runtime — two recorders would be exactly the fork both phases warn
about. **Coordinate with phase-236 before starting.**

## Waves

### W1 — the probe driver (NEXT)

Two prerequisites found while landing W2, both concrete:

1. **The CMake parser drops `HEADER` and `SHAPE`.** `CmakeNodeSummary`
   carries package / component / executable / class / language / deploy, but
   `nros_components_register_node` also accepts `HEADER <hdr>` and
   `SHAPE rclcpp|configure`. The probe needs both: the header to `#include`,
   and the shape to know whether to call `configure(node)` or construct
   through the rclcpp-compat factory. Extend
   `discover_cmake_node_metadata` first.
2. **Header convention as the fallback.** With no explicit `HEADER`, the
   in-tree shape is `<pkg>/include/<pkg>/<Class>.hpp` — derivable from the
   class (`talker_pkg::Talker` → `talker_pkg/Talker.hpp`). Use the declared
   `HEADER` when present; never guess silently past that.

Then mirror `metadata_build.rs`, with CMake in place of cargo: generate a probe
project that `add_subdirectory`s the node package, links it plus `nros-cpp`
built with `metadata-mode`, constructs the class, runs its declaration path
against the recorder, and calls `nros_cpp_metadata_dump`. Select the backend
with `NROS_RMW=metadata`.

The probe binary must run with `NROS_RMW=metadata` set, or the executor picks
whatever real backend the component's own dependencies registered and the
probe tries to open a transport.

### W1 — the probe driver (original text)

Mirror `metadata_build.rs`: generate a host probe that instantiates the node
class named by the cmake `nano_ros_node_register(CLASS …)` summary, runs its
declaration path against the recorder, and writes the sidecar. The CLI already
carries the C/C++ summaries (`cmake_component_metadata`) and already makes them
component declarations — phase-307 W5's gate asserts it — so the discovery half
is done; only the build+run half is missing.

### W2 — the adapter — DONE (2026-07-26)

Landed in four commits:

- **W2a** `nros::metadata_mode` — the process-global recorder non-Rust
  adapters feed. The global is the only new thing: same `MetadataRecorder`,
  same `push_node` / `push_entity`, same `to_source_metadata_json`.
  `push_node` / `push_entity` / `entity_metadata` / `EntityMetadataSpec` went
  `pub(crate)` → `pub` as the adapter surface, and `SourceMetadataExport`
  gained `language` (it was a hardcoded `"rust"` literal in the serializer
  while Rust was the only producer).
- **W2b** `nros-rmw-metadata` — the recording backend. Written as a Rust
  `Rmw`/`Session` impl and adapted by `RustBackendAdapter`, so it is ~15 tiny
  functions rather than a 38-slot hand-written vtable. Registered by NAME, not
  as default: a probe may well link a real backend too (the component's deps
  pull one in), and registering as default would make the choice ambiguous and
  the executor would refuse to open at all.
- **W2c** the three `nros-cpp` hooks — timers, guard conditions, and node
  identity. The first two never reach the RMW; the third is the finding that
  changed this design: `create_publisher(session, topic_name, type_name, …)`
  carries **no node**, so a backend alone yields a sidecar whose entities
  belong to no node.
- **W2d** `nros_cpp_metadata_dump` — exported from `nros-cpp` so the probe TU
  links one Rust staticlib. Recording NOTHING returns an error rather than
  writing an empty sidecar.

Layer discipline holds: neither `nros-rmw-metadata` nor `nros-cpp`'s hooks
contain JSON, a schema struct, or slot arithmetic.

**Failure policy throughout:** a refused record fails the create (backend) or
panics (hooks). A dropped entity is an under-counted sidecar is an under-sized
executor at boot — a probe that lies is worse than a probe that fails.

### W3 — schema parity, by construction rather than by test

The emitted sidecar is byte-schema-identical to the Rust one because it comes
out of the same serializer, not because a test compares two of them. Same
`(package, executable)` key, so the consumption side needs no language branch —
`nros_orchestration_ir::sidecar_slots` already counts C++ sidecars correctly the
day they appear, with no change.

Also close phase-307's documented-but-unenforced contract here: a component
whose DECLARATIONS differ under `#ifdef` / `#[cfg]` between host and target
records a count that does not describe the firmware. The rule is *declare
unconditionally, gate behavior not declaration*, and C++ makes it easier to
break than Rust does. Detect and fail loud rather than silently skew a count.

### W4 — close the ledger

`nros sync` stops reporting "no producer for …"; phase-307's
`cpp_producer_gap_is_tracked_not_hidden` asserts zero.

## Acceptance

- [ ] Every C and C++ node package in `examples/` produces a schema-valid
      sidecar through the same discovery path and schema as Rust.
- [ ] The consumption side has no language branch (it should need no edit at
      all — `sidecar_slots` counts a C++ sidecar the day it appears).
- [ ] Phase-307's producer-gap ledger test asserts zero unsupported packages.
- [ ] The recording mode cannot be reached from a firmware build.
- [ ] `nros-cpp`'s recording path contains no JSON, no schema struct and no
      slot arithmetic — the layer boundary holds by inspection.
- [ ] A cfg-divergent declaration fails loud instead of recording a count that
      does not describe the firmware.
