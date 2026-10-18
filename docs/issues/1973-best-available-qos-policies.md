---
id: 1973
title: "Iron/Jazzy BEST_AVAILABLE QoS policies and rmw_qos_profile_best_available are not expressible"
status: open
type: enhancement
area: rmw
severity: low
found: 2026-10-19
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
