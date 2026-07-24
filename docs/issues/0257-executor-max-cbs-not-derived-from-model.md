---
id: 257
title: "NROS_EXECUTOR_MAX_CBS is a hidden compile-time env the entry codegen could derive from the model"
status: open
type: enhancement
area: build
---

## Finding (autoware-safety-island-example P2/P3, 2026-07-24)

The executor callback-slot count is `NROS_EXECUTOR_MAX_CBS` (nros-node
build.rs env, default 4). A 3-node workspace registers ~9 entries → boot
died `create_timer (code=-6 Full)` with no build-time hint; the 4-node
island needed 32. Users discover the knob at runtime, and changing it
resizes the executor arena — mixed stale objects then SEGV in shutdown
(clean rebuild required).

The entry codegen consumes the SystemModel and KNOWS the per-node entity
counts (subs + services + timers + clients). It should derive the knob (or
at minimum emit a static_assert-style configure-time check: model entities
vs compiled capacity) instead of shipping a silent default-4.

Same class: `NROS_CYCLONEDDS_MAX_TYPES` (Rust registry) and the C++
descriptor registry cap (was silently-overflowing 64; raised + override in
the #253-adjacent fix).
