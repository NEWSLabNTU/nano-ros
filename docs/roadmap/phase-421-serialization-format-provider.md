# Phase 421 — serialization format as a compile-time provider

**Status (2026-09-04). W1–W3 landed; W4–W6 open.**
Implements [RFC-0088](../design/0088-serialization-format-is-a-compile-time-provider.md).
Depends on **[phase-420](phase-420-package-identity-and-provider-format.md) W1
only** (the `<nano_ros_uses>` general consumption tag) — and only from W4 onward;
W1–W3 here are pure Rust and need nothing from that phase.

Related: RFC-0009 (bridge topic forwarding), RFC-0011 (uORB backend), RFC-0035
(RMW vtable ABI), RFC-0054 (C headers are the ABI SSoT), phase-325 (uORB interop
and bridge).

## Goal

Turn three unchecked prose claims into checked facts, and make a user's
serialization library a provider package.

The claims today:

- `EmbeddedRawPublisher::publish_raw` — *"Publish raw CDR-encoded data (must
  include CDR header)"*, checked by nothing.
- `nros-bridge` — *"both sides must use ROS-CDR … Cross-encoding bridges would
  need an explicit translator and are out of scope"*, checked by nothing.
- uORB, whose wire is the PX4 struct verbatim, treated as a special case rather
  than as a backend whose format differs.

The mechanism is compile-time, not runtime: ROS 2 uses format strings because
`rosidl_typesupport_c` resolves them through `dlopen`, and we do not `dlopen`.

## Work items

- [x] **W1 — the format is a compile-time fact.** (landed 2026-09-04)
      `nros_serdes::format` carries `SerializationFormatId` (`#[repr(u8)]`), the
      `SerializationFormat` trait and the `Cdr` / `Uorb` markers.
      `nros_core::RosMessage` gains a **defaulted** `SERIALIZATION_FORMAT_ID`;
      `nros_rmw::Session` gains `SERIALIZATION_FORMAT` + `_ID`, also defaulted;
      `nros_node::session` exposes `IMAGE_SERIALIZATION_FORMAT{,_ID}`.
      `format_check::{assert_message_format, assert_raw_format}` compare inside
      an inline `const {}`, called from the four typed entity-creation funnels.
      See "What landed differently" for why this is a const on `RosMessage`
      rather than an associated type on `schema::Message`.
      **Acceptance, met:** a `Uorb` message in a CDR image fails to build with
      `error[E0080]: evaluation panicked: message serialization format does not
      match the linked backend (RFC-0088)` plus the instantiation note naming
      the type — measured with a probe, which was then removed. No branch is
      added to the publish path: the whole effect is the `const {}`.

- [x] **W2 — the reserved vtable slot gets a body.** (landed 2026-09-04) `get_serialization_format`
      leaves the `identity` inert family in `check-rmw-slot-producers.py`; every
      backend returns its constant. Add the per-session accessors:
      `Session::serialization_format()` and `Node::serialization_format()` in
      Rust, `nros_node_get_serialization_format()` in C (cbindgen output), and
      `nros::Node::serialization_format()` returning `const char*` in C++.
      **Acceptance:** `check-rmw-api-parity` classifies the slot as produced; a
      two-backend image reports two different formats from two sessions.

- [x] **W3 — the bridge becomes the only runtime site.** (landed 2026-09-04) `RawSubscription::format()`;
      `PubSubBridge::new` returns `Result<_, BridgeError>` and refuses a
      mismatch; `SerializationFormatConverter` plus
      `PubSubBridge::with_converter` for the deliberate cross-format case.
      **Acceptance:** a bridge wired zenoh→uORB without a converter fails at
      construction with both format names in the error; the existing
      `declarative_bridge_zenoh_to_{cyclonedds,xrce}` cells stay green; the
      comparison is one byte, at construction, not per sample.

- [ ] **W4 — the `serdes` provider family.** One `FAMILIES` row in
      `check-provider-announcements.py`; `nros-serdes.toml` whose only field is
      `impl = "schema" | "codegen"` (absent file means `schema` and all
      defaults); `<nano_ros_provides kind="serdes" name="…"/>`; consumption via
      `<nano_ros_uses kind="serdes" name="…"/>` (phase-420 W1) and a
      `[deploy.<t>] serdes = "…"` key; `check-serdes-descriptors` covering S1–S4
      plus discriminant allocation.
      **Acceptance:** a provider package outside the repo is selected by name and
      reaches the build; no parser learns a new attribute to make that work.

- [ ] **W5 — the schema-driven strategy, with a reference provider.**
      `SchemaSerializer` over the `&'static [Field]` schema codegen already
      emits. Ship one in-tree non-CDR reference provider so the path has a test
      subject that is not uORB (uORB's wire is a struct, so it cannot exercise a
      schema walk).
      **Acceptance:** the reference format round-trips every message in
      `packages/interfaces/*` through the schema walk with no generated code, and
      a native matrix cell covers it. **No new matrix coordinate** — one cell, not
      an axis (RFC-0088 non-goals).

- [ ] **W6 — C and C++ assert at compile time.** `NROS_SERIALIZATION_FORMAT_ID` /
      `NROS_SERIALIZATION_FORMAT` in the generated config; a per-message
      `_Static_assert` in C; `format_of<M>` plus `static_assert` in C++, using
      `const char*` rather than `std::string_view` so the header survives Zephyr's
      minimal libcpp. `check-format-macro-scope`: a bridge-linked image must not
      reference the macro, because a two-backend image has no single answer.
      **Acceptance:** a C entry publishing a message whose format differs from
      the linked backend fails to compile; a bridge image referencing the macro
      fails the gate.

## What landed differently from the plan (2026-09-04)

- **W1's mechanism changed and RFC-0088 D1 was amended to match.** The format is
  a defaulted const on `nros_core::RosMessage`, not an associated type on
  `nros_serdes::schema::Message`. The associated type was implemented first
  (142 impls, 95 files) and reverted: `MessageForRmw` requires a schema only
  under `cfg(rmw_needs_type_descriptors)`, so the check was absent under zenoh,
  XRCE and uORB — the last being the backend it exists for. The const version
  touches no existing implementor and is universal.
- **The check is an inline `const {}`, not a where-clause bound** — `NodeHandle`
  is not generic over the backend. It is a `cargo build` error, not `cargo
  check`: the monomorphisation collector runs only during codegen, and the probe
  must be reachable (a private never-called fn is dropped first).
- **The raw path is an explicit `assert_raw_format::<F>()`**, because rustc
  forbids a defaulted type parameter on a function.
- **W2 found the uORB vtable truncated.** Its positional initializer stopped at
  `destroy_client`; reaching the new slot means naming 21 intervening slots,
  each written as the `nullptr` C++ was already giving it.
- **W2 also found `check-abi-bindings` had been silently skipping** — `bindgen-cli
  0.72.1` was not installed on the host, so the gate never ran.
- **W3's runtime path is not yet reachable.** Both `format()` accessors answer
  from the single image constant, so `PubSubBridge::new` cannot fail in a real
  image until a bridge image links two backends. The error rendering is tested;
  the end-to-end case waits on W4/W5.

## Sequencing

W1 → W2 → W3 are independent of phase-420 and can land immediately. W4 needs
phase-420 W1. W5 needs W4. W6 needs W1 and W2.

## Risks

- **Codegen touches every generated message.** W1 adds one associated type per
  message; regenerate and diff rather than hand-editing (`packages/interfaces/*`
  are committed).
- **The `u8` is image-local (RFC-0088 D2).** The temptation to treat it as a
  global registry will recur; the string is the cross-image identity, and a
  reviewer should reject any use of the discriminant in a file that outlives one
  build.
- **Schema-driven serialization is slower than generated.** Deliberate for v1;
  the answer is `impl = "codegen"`, which needs a codegen plugin ABI and is
  therefore out of scope here.
- **`can_loan_messages` (issue 0814) overlaps.** Loaning is the sanctioned
  zero-serialization path; this phase must not add a format check on a path that
  never serializes.

## Out of scope

Per-topic or per-publisher serializer selection (ROS 2 does not offer it
either); a codegen plugin ABI; a new matrix axis; converters as provider
packages.
