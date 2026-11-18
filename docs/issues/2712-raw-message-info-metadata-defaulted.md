---
id: 2712
title: "The generic `.message_info()` subscription reads every metadata field as zero, and no C call can deliver a message's metadata"
status: open
type: bug
severity: medium
area: [api, executor, rmw, c-api]
found: 2026-11-18
related: [2349, 2036]
---

**Home: [issue 2349](2349-component-api-exposes-no-message-info.md)**, whose C
half needs this fixed first.

## Symptom

A generic subscription built with
`node.subscription(t).generic(..).message_info()` gets a `RawMessageInfo`
whose metadata accessors (`publication_sequence_number`, `publisher_gid`,
`source_timestamp`, `received_timestamp`) all read zero for every message.
They look like real values and are not.

Measured 2026-11-18: issue 2349's first lowering used this path, and against
a live talker over zenohd every message printed
`seq=0 gid=000000000000000000000000000000000000000000000000 ts=0 rx=0`.
The same talker through `add_arena_subscription_msg_info_closure` gave a
rising `seq`, a stable non-zero GID and both timestamps.

## Why

The entry (`SubBufferedRawInfoEntry`, `register_subscription_buffered_raw_info_on`)
takes with `take_serialized_with_attachment` and builds
`RawMessageInfo::new(attachment)`, which defaults the metadata by
construction. The metadata comes only from `take_serialized_with_info`, a
second take this path never makes.

The attachment half is empty as well. No backend overrides
`take_serialized_with_attachment`, and the C vtable has no attachment slot,
so the trait default reports `attachment_len == 0` everywhere. The bridge's
wire-level echo check never fires; its FNV payload-hash fallback is what
suppresses echoes today (`nros-rmw-bridge`'s `pump_with_stats`).

## The C side

The C calls named for metadata carry none.
`rclc_executor_add_subscription_raw_with_info` and
`rclc_executor_add_subscription_with_info` both register this entry, and
their callback type, `rclc_subscription_info_callback_t`, is `(data, len,
attachment, attachment_len, context)`: it has no parameter a sequence number
or GID could travel in. So a C program has no way at all to read a message's
metadata, and the name says it does.

## Fix direction

- One take that returns both the metadata and the attachment, so this entry
  fills `RawMessageInfo` from real metadata and keeps the attachment for the
  bridge. A trait method whose default makes the metadata take (the half
  every backend that reports metadata already implements) loses nothing
  today, because no backend delivers an attachment.
- C needs a callback that can carry the metadata. That is an API decision,
  not a bug fix: either the existing `rclc_subscription_info_callback_t`
  gains a metadata parameter (an ABI break on a type kept for ABI
  compatibility), or a new callback type and registration carries it.
- A test that fails on `seq=0`/`gid=0` for each road, as
  `tests/component_message_info.rs` does for the component API.

## Progress (2026-11-18)

**Rust: fixed.** `Subscription::take_serialized_with_info_and_attachment`
takes one sample with both its metadata and its attachment. Its default
makes the metadata take and reports no attachment, which loses nothing,
since no backend delivers one. The generic `.message_info()` entry takes with
it and builds `RawMessageInfo::with_info`, so its metadata accessors read
what the backend reported.

Tests:
- `executor::tests::a_generic_message_info_subscription_reads_the_backends_metadata`:
  a mock backend reports timestamps, and the callback must see them. Red
  against the old construction (`source timestamp ... left: Time { sec: 0,
  nanosec: 0 }`).
- `take_with_info.rs::the_combined_take_carries_the_backends_metadata`: the
  same over the C vtable the runtime uses.

**C: open, ruled (ii)** (manager, 2026-11-18). A new callback type and
registration carrying `const rmw_message_info_t *`. The two attachment-only
`*_with_info` calls are marked deprecated, naming it; they are not removed.

