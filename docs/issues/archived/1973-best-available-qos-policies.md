---
id: 1973
title: "Iron/Jazzy BEST_AVAILABLE QoS policies and rmw_qos_profile_best_available are not expressible"
status: resolved
type: enhancement
area: rmw
severity: low
found: 2026-10-19
resolved: 2026-10-20
related: [rfc-0100]
---

## What

Iron added `RMW_QOS_POLICY_{RELIABILITY,DURABILITY,LIVELINESS}_BEST_AVAILABLE`,
`RMW_QOS_DEADLINE_BEST_AVAILABLE`, `RMW_QOS_LIVELINESS_LEASE_DURATION_BEST_AVAILABLE`
and the profile `rmw_qos_profile_best_available`; rclcpp exposes
`rclcpp::BestAvailableQoS`, `QoS::reliability_best_available()`,
`QoS::durability_best_available()`, and rclrs `QoSProfile::best_available` plus
a `BestAvailable` variant on each policy enum. None exist here.

Semantics: a SUBSCRIPTION with a best-available policy resolves it at creation
to the most demanding value every currently-discovered publisher offers, so it
is compatible with all of them (and the reverse for publishers).

## Ruling: ADOPT, bounded

Resolution needs the graph at entity-creation time, which the zenoh graph
cache and Cyclone's builtin readers already provide; a backend without a graph
resolves to the policy's documented fallback (reliable / transient-local /
automatic), which is what upstream does when nothing is discovered yet.
Depth is not a best-available field, so RFC-0100's sizing (depth x payload)
is unaffected.

## Work

1. Policy enum values + the profile constant (the QoS-profile SSoT gate
   `check-qos-profile-ssot` gains the Jazzy record).
2. Resolution in the backends' create paths, reported by `*_get_actual_qos`.
3. C++ `BestAvailableQoS` / the two `QoS` setters; Rust `QoSProfile` and
   enum variants.
4. Delete the `gap` rows.

## Resolution (2026-10-20)

Adopted across the rmw ABI, the core Rust types, C, C++ and the Rust API, with
ONE resolution rule shared by every backend.

**The rule** is `nros_rmw::QoSProfile::resolve_best_available`, upstream's
`rmw_dds_common` `qos_profile_get_best_available_for_{subscription,publisher}`
and `qos_profile_update_best_available_for_services` transcribed: a
subscription takes RELIABLE / TRANSIENT_LOCAL / MANUAL_BY_TOPIC only when at
least one publisher is discovered and every one offers it (else BEST_EFFORT /
VOLATILE / AUTOMATIC) and the LARGEST finite peer deadline and lease; a
publisher always takes RELIABLE and TRANSIENT_LOCAL, takes MANUAL_BY_TOPIC if
any subscription asks for it, and the SMALLEST finite peer deadline and lease;
a service or client takes `rmw_qos_profile_services_default`'s values. A peer
whose profile the backend cannot read contributes its least demanding value.
The choice is then CLAMPED to the backend's `supported_qos_policies` mask
(TRANSIENT_LOCAL → VOLATILE, MANUAL_BY_TOPIC → AUTOMATIC, a finite window →
default, and the `SYSTEM_DEFAULT` sentinel where the policy is not honoured at
all), so "best available" never chooses a value the next line refuses. Peers
are a fold (`BestAvailablePeers`), so no allocation.

**Where it runs**: `nros_rmw::resolve_best_available_on(session, role, topic, qos)`
walks `Session::get_endpoint_info_by_topic` for the other side of the topic and
applies the rule. It is called at the ONE seam every backend's create passes
through — `nros-rmw-cffi`'s `CffiSession::create_{publisher,subscription,
service,client}` (`nros-node`'s session IS that type) — before the profile is
lowered for the backend's create slot. So no backend's create slot receives a
best-available value, and `*_get_actual_qos` reports the concrete grant.
`ZenohSession`'s create paths call the same helper (a no-op behind
`CffiSession`; it keeps a direct `ZenohSession` caller from putting a request
value into the liveliness token), and zenoh's `qos::admit` refuses an
unresolved value rather than folding it.

**Per backend**:

- **zenoh** — resolves against its graph cache: `GraphEndpointInfo` gained a
  `qos` field, filled from each liveliness token's QoS chunk by
  `keyexpr::qos_from_keyexpr` (upstream `rmw_zenoh_cpp` grammar, empty field =
  its `default_qos`). A cache that is not running yet, or
  `ZPICO_GRAPH_DISCOVERY=0`, is "nothing discovered".
- **cyclonedds** — resolves against its builtin readers: its
  `get_*_info_by_topic` already filled `qos_profile` from the discovered
  endpoint's DDS QoS, and `CffiSession`'s trampoline now carries it into
  `GraphEndpointInfo::qos`.
- **xrce, uorb** — no graph slot, so "nothing discovered": upstream's own
  answer for that case (subscription BEST_EFFORT / VOLATILE / AUTOMATIC,
  publisher RELIABLE / TRANSIENT_LOCAL / AUTOMATIC), clamped to each backend's
  mask (uORB serves VOLATILE only; XRCE honours no liveliness, so liveliness
  lands on the sentinel). Documented on `resolve_best_available`.

**ABI** (append-only): `NROS_RMW_RELIABILITY_BEST_AVAILABLE` (4),
`NROS_RMW_DURABILITY_BEST_AVAILABLE` (4), `NROS_RMW_LIVELINESS_BEST_AVAILABLE`
(5), and `NROS_RMW_DURATION_BEST_AVAILABLE_MS` (`UINT32_MAX - 1`, upstream's
`RMW_DURATION_INFINITE - 1`) for `deadline_ms` / `liveliness_lease_ms`; no
finite duration lowers onto it (`duration_to_qos_ms` and C++'s
`qos_window_ms` saturate below it). Bindings regenerated. The C API
(`RCLC_QOS_*_BEST_AVAILABLE`) and C++ FFI (`NROS_CPP_QOS_*_BEST_AVAILABLE`)
enums gained the same appended values.

**Surfaces**: Rust `BestAvailable` on the three policy enums,
`QoSProfile::QOS_PROFILE_BEST_AVAILABLE` (a `qos_profiles!` row) and rclrs's
`QoSProfile::best_available()`; C++ `ReliabilityPolicy` / `DurabilityPolicy` /
`LivelinessPolicy::BestAvailable`, `QoS::reliability_best_available()`,
`QoS::durability_best_available()` and `rclcpp::BestAvailableQoS` (a
`qos_table` row).

**SSoT gate**: `check-qos-profile-ssot` gained the Jazzy record
(`docs/reference/rmw-qos-profiles-jazzy.txt`, extracted from jazzy rmw headers
— merged with the Humble base, each symbol in one file) and binds the Rust row
and the C++ `qos_table` row to it; the header extractor resolves the Iron+
best-available windows onto our sentinel after checking they are
`RMW_DURATION_INFINITE - 1`.

**Ledgers**: the four `gap` rows are gone (they match under Jazzy); the same
names carry Humble-scoped `extension` rows, since Humble predates them. The six
`rmw-api-map.toml` Jazzy type-delta rows are `re-mapped` onto the new ABI
values. `has_best_available` / `resolve_best_available` are `extension` rows.

**Bounded, deliberately**: like upstream, the choice is made once at creation
and is not revisited as endpoints come and go. C++ has no public spelling for
an individual best-available deadline or lease (upstream's takes an
`rmw_time_t` this surface does not carry) — `BestAvailableQoS` sets both; Rust
writes `DURATION_BEST_AVAILABLE_MS` into the field. `qos_overrides.*`
parameters do not parse `best_available` (an override applies a stated value).

Tests: `nros-rmw` `best_available_tests` (the rule, case by case, incl. no
peers, unreadable peers, services, clamping, stated fields untouched, the
duration sentinel); `nros-rmw-cffi`
`best_available_reaches_the_backend_resolved_against_its_graph` and
`best_available_without_a_graph_slot_resolves_to_nothing_discovered` (what the
backend's create slot RECEIVES); `nros-rmw-zenoh` keyexpr round-trip/decode
tests; `qos_policy_accessors.cpp` static_asserts for the C++ surface.

