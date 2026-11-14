---
id: 2530
title: "Does the PlatformIO integration get the codegen version anchor?"
status: open
type: design-question
severity: low
area: [codegen, integrations]
related: [rfc-0090]
found: 2026-11-14
---

**Moved here from RFC-0090 open question 3** when the RFC became Stable
(phase-555 W3, rule A1(a)).

RFC-0090 makes the codegen version the compatibility token: generated code
carries the version it was emitted for, and a consumer refuses a mismatch. The
PlatformIO integration under `integrations/` emits with a soft failure today.
Should it get the anchor too?

The ESP-IDF half of the question is moot: that integration was deleted in
phase-468 W2, and RFC-0065 D3 removed its build road.
