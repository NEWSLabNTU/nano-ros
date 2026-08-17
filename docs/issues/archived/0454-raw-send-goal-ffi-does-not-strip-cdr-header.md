---
id: 454
title: "the `*_send_goal_raw` C/C++ FFIs take a param named `goal_cdr` but never strip its header, so `PollingActionClient` would ship the #448 double encapsulation"
status: resolved
type: bug
area: api
related: [issue-0448, issue-0418, issue-0656, rfc-0069, phase-354]
---

## The inconsistency

Four entry points reach `ActionClientCore::send_goal_raw`, whose contract is
"the goal FIELDS, no encapsulation header — I frame `[header][goal_id]` myself":

| Entry point | Strips? |
| --- | --- |
| `nros_action_client_send_goal` (`nros-c/src/action/client.rs:795`) | **yes** — `let goal_fields = strip_cdr_header(goal_data);` |
| `nros_cpp_action_client_send_goal` (`nros-cpp/src/action.rs:961`) | **yes** |
| `nros_cpp_tick_ctx_send_goal` blocking sibling (`action.rs:1550`) | **yes** |
| `nros_action_client_send_goal_raw` (`nros-c/src/action/client.rs:1196`) | **no** |
| `nros_cpp_action_client_send_goal_raw` (`nros-cpp/src/action.rs:2478`) | **no** |

The two `_raw` ones take a parameter named `goal_cdr` — the SAME name the
stripping variants use for their `[CDR_HEADER][fields]` input — and pass it
through untouched. Their doc comments say nothing about framing:

```c
/**
 * Phase 122.3.d — L1 polling: send a goal. Writes 16-byte UUID
 * into `goal_id_out`.
 */
nros_cpp_ret_t nros_cpp_action_client_send_goal_raw(void *storage,
                                                    const uint8_t *goal_cdr, ...);
```

## Why it matters

`PollingActionClient<A>::send_goal` — a public templated C++ API — serializes
with `ffi_serialize`, which emits `[CDR_HEADER][fields]`
(`packs/cpp/message_exports.rs.jinja` uses `CdrWriter::new_with_header`), and
hands the result straight to the non-stripping `_raw`:

```cpp
uint8_t buf[GoalType::SERIALIZED_SIZE_MAX];
size_t len = 0;
if (GoalType::ffi_serialize(&goal, buf, sizeof(buf), &len) != 0)
    return Result(ErrorCode::Error);
return Result(nros_cpp_action_client_send_goal_raw(
    storage_, buf, len, reinterpret_cast<uint8_t(*)[16]>(goal_id_out)));
```

That is exactly issue 0448 — two encapsulation headers on the wire
(`encap|uuid|encap|fields`), 4 bytes over the ROS 2 `SendGoal_Request` layout,
which Fast-DDS drops outright.

## Why it is not currently failing

`PollingActionClient` has **no consumer**: it appears only in its own header,
`node.hpp`, and `action.rs`. No example and no test instantiates it, and neither
`_raw` FFI is called from `examples/` or `packages/testing/`. So this is latent,
not live — the C++ examples use `nros::ActionClient`, whose `send_goal` calls
the STRIPPING `nros_cpp_action_client_send_goal`, and the cpp action cells
deliver `order` correctly.

Found by sweeping the 0448 class rather than by a failure, which is the point:
the live Rust site and this latent C++ site are the same defect, and only one of
them had a test that could see it.

## Fix options

1. **Make the two `_raw` FFIs strip**, matching every sibling in the same files
   and the `goal_cdr` parameter name. Safe today precisely because there are no
   other callers — but it is a public C ABI, so the doc comment must state the
   framing either way.
2. **Strip in `PollingActionClient::send_goal`** and document `_raw` as
   fields-only. Keeps `_raw` genuinely raw, at the cost of the two spellings
   that CLAUDE.md warns about (a second idiom instead of a shared helper).

(1) is preferred: one rule — "every `send_goal` entry point takes
`[CDR_HEADER][fields]` and strips" — instead of a per-entry-point convention
nobody can infer from the signature.

**Whichever is chosen, it needs a consumer to verify.** The reason this sat
undetected is that no test instantiates `PollingActionClient` at all; fixing the
strip without adding a cell would just move an unverified claim.

## Resolved 2026-08-17 (phase-354 W3)

Both FFI arms strip: `core.send_goal_raw(strip_cdr_header(slice))` in
`nros-c/src/action/client.rs` and `nros-cpp/src/action.rs`. Guarded by
`scripts/check-goal-cdr-stripped.py`.

The acceptance asked for a WIRE demonstration, and that needed the caller this
FFI never had — `packages/testing/nros-tests/bins/action-raw-goal-probe`, a
CMake C leaf with its own `[[fixture]]` row, driven by
`tests/action_raw_goal_e2e.rs`. It asserts on the C action SERVER's decoded
order, not the probe's own account.

**Falsified before being trusted.** With the strip removed and rebuilt, the
server reports order **256** against the 7 sent; restored, the test passes.
256, not the 65536 this issue's arithmetic would predict — the parsed request is
`[encap][GoalId(16)][order]`, so the four extra bytes shift the tail rather than
landing in `order` directly.

Writing the caller surfaced two further defects: the polling arms never received
phase-338 W3's channel-type fix (fixed in the same change, at all six remaining
sites), and actions ignore `ROS_DOMAIN_ID` entirely — filed as
[issue 0656](0656-action-raw-register-ignores-ros-domain-id.md).
