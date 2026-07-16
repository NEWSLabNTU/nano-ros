---
id: 212
title: "workspace custom-msg C examples hand-roll CDR (fixed offsets + hand-typed DDS type name) — no generated C typesupport for workspace messages"
status: resolved
type: feature-gap
area: codegen
related: [issue-0203]
---

## Problem (audit 2026-07-16, J1)

`examples/workspaces/ws-custom-msg-c/src/reading_talker_pkg/src/ReadingTalker.c:42`
(and the mixed variant's C side): the example manually writes the 4-byte CDR
encapsulation header, memcpys temperature/humidity/sequence at fixed byte
offsets, and hand-types "custom_msgs::msg::dds_::Reading_". Codegen emits no
C serialization/typesupport for workspace-local custom messages, so the
copy-out example teaches users to hand-roll wire format — offset drift on
any msg change.

## Fix sketch

Extend `nros generate` C emission to workspace custom msgs (serialize/
deserialize + TYPE_NAME consts), then rewrite the examples on it. Sibling of
#203 (cpp interface over-generation) on the same codegen surface.

## Resolution (2026-07-16, phase-293)

Two wiring gaps fixed (no new generator needed):

1. `nros codegen resolve-deps` now layers `NROS_INTERFACE_SEARCH_PATH`
   workspace roots above the ament env index + bundled interfaces
   (`load_index_with_fallback`, cargo-nano-ros) — a node whose package.xml
   depends on a sibling workspace msg pkg finally resolves it.
2. `nros_find_interfaces` (cmake) injects the workspace search-path var into
   the CLI child env via `cmake -E env` (caller-exported env still wins).

All three ws-custom-msg workspaces rewritten onto GENERATED bindings
(C struct+serialize/deserialize+type-name; cpp typed Publisher<Reading> +
bind_subscription member callback). The feared double-builtin_interfaces
cpp glue edge did NOT reproduce (killed by the #203-era repairs). Verified:
manual pairs 9/9 with correct values ×2; nextest custom_msg suite +
c/cpp/mixed workspace e2e = 10/10 on rebuilt fixtures.

Residual: the STD-msgs raw-CDR components (workspaces/c, ws-qos-*,
templates, ws-realtime-* C pkgs) are the same antipattern at wider scope —
issue #218. Phase doc: docs/roadmap/phase-293-c-cpp-custom-msg-typed-bindings.md.
