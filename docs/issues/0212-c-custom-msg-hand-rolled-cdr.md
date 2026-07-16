---
id: 212
title: "workspace custom-msg C examples hand-roll CDR (fixed offsets + hand-typed DDS type name) — no generated C typesupport for workspace messages"
status: open
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
