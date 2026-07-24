# Phase 301 — pre-release RMW shape alignment (resolves 0240 + 0241)

**Status (2026-07-24): Draft — waves W1–W5 below.** Implements the batched
API+ABI cleanup issues 0240 (naming/shape drift from `rmw.h`) and 0241
(silent-lossy QoS boundary conversions) as ONE deliberate break, on the
phase-299 (RFC-0054) header-SSoT model: every change is authored in
`packages/core/nros-rmw-abi/include/nros/*.h`, Rust regenerates via
`scripts/gen-abi-bindings.sh`, and the regen-diff gate enforces sync.

**Why now (the release window).** The RFC-0054 headers are the published
contract external consumers will pin at first release. Pre-release, this
break is a header edit + regen + call-site sweep; post-release every rename
is a compatibility event. The old cost argument (hand-mirror churn + drift
gates) evaporated with phase-299.

**Decisions baked into this phase (recorded here, argued in 0240):**

- **`session` STAYS `session`** — 0240 floated `nros_rmw_node_t`, but
  post-297 multi-tier runs MULTIPLE node-executors over ONE shared RMW
  session; the struct names the connection scope, not a node. The
  `node_name`/`namespace_` fields are the primary session-node's identity
  (documented as such in the header). This is the one 0240 rename we
  REJECT, with rationale.
- **`open`/`close` → `create_session`/`destroy_session`** — restores the
  table's own `create_*`/`destroy_*` convention.
- **`subscriber` → `subscription`** (types, slots, headers, backends) —
  rmw's term; gratuitous divergence removed.
- **`service_server` → `service`, `service_client` → `client`** — rmw's
  `rmw_service_t`/`rmw_client_t` terms.
- **Transport hints leave the QoS struct**: new
  `nros_rmw_publisher_options_t` (carries `tx_express`) and
  `nros_rmw_subscription_options_t` (carries `rx_buffer_hint`), passed as
  a NULLable trailing param to `create_publisher`/`create_subscription`.
  `nros_rmw_qos_t` becomes a pure policy mirror; hint growth no longer
  churns the QoS ABI.
- **`call_raw` DELETED** — the deprecated blocking RPC slot goes; the
  async `send_request_raw` + `try_recv_reply_raw` pair is the one path.
- **0241 boundary semantics**: `depth > 65535` is a CREATE-TIME error
  (`NROS_RMW_RET_INVALID_ARGUMENT`), never a silent saturate. Duration
  fields adopt an explicit sentinel: `NROS_RMW_DURATION_INFINITE`
  (`UINT32_MAX` ms) = infinite/unset; `0` = a real zero. Sub-ms
  truncation is documented at the header (representable range is part of
  the contract); a duration that truncates to a DIFFERENT value than
  requested at ms resolution is accepted (documented floor), but overflow
  past the u32-ms range is an error, not a clamp.
- **Return-code scheme unchanged** (0240's own carve-out: the
  negative-error/byte-count dual convention is justified).

## Waves

### W1 — header break (`nros-rmw-abi`)

Author every rename + reshape in the SSoT headers: verb renames in
`rmw_vtable.h`, type renames in `rmw_entity.h`/`rmw_event.h`/`rmw_ret.h`
doc text, the two options structs, `call_raw` slot removal,
`NROS_RMW_DURATION_INFINITE`, and the boundary-semantics doc comments.
Bump the ABI version constant. Regen (`scripts/gen-abi-bindings.sh`) +
commit the generated bindings in the same change.

### W2 — Rust core (`nros-rmw-cffi` + trait layer)

Adapt to the regenerated bindings: rename trait methods/types to match,
delete the deprecated `call_raw` trait path, thread the options structs
through `create_publisher`/`create_subscription`, and implement the 0241
validation in the `QosSettings` lowering — out-of-range depth/duration →
create-time error; `0`-vs-infinite disambiguated via the new sentinel.
Unit tests for each rejected boundary.

### W3 — backends + language layers

Mechanical rename + signature sweep: `nros-rmw-zenoh` (Rust),
`nros-rmw-cyclonedds` (C++), xrce, uorb, plus `nros-c`/`nros-cpp` and any
generated-entry emitters that spell the old names. No behavior change
beyond W1/W2 semantics.

### W4 — recorded carve-outs + author guidance (rides the phase)

- 0242: GID + message-info carve-out rationale into
  `book/src/design/rmw-vs-upstream.md` (demand-driven optional slots
  later; the extension pattern keeps that ABI-safe).
- 0243: one paragraph of board-author guidance (implement
  `nros-platform::board`, legacy family is transition-only).
- 0240/0244 recalibration notes already landed.

### W5 — verification

`just check` + regen-diff gate green; rebuild affected fixture families;
run the RMW-touching lanes (native pubsub/service/action ×3 langs, one
embedded zenoh lane, one cyclone lane). Resolve + archive 0240/0241.

## Coordination

The 296 session works the same backends; land W1+W2 in one push window and
announce the rename set (this doc) so in-flight branches rebase once.

## Non-goals

- 0242's optional slots (demand-driven follow-up).
- 0243's full trait-family convergence (sequenced with phase-230).
- Any wire-protocol change — this is API/ABI naming + boundary semantics.
