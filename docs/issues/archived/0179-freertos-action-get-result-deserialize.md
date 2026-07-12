---
id: 179
title: "zenoh C/C++ action e2e (ALL platforms incl. native): goal + feedback deliver, get-result reply fails to deserialize"
status: open
type: bug
area: rmw-zenoh
related: [phase-287, issue-0135]
---

## Summary

On the phase-287 W6 native-identical freertos leaves (standalone
`nros_app_main` + rmw-zenoh-cffi/zenoh-pico), the Fibonacci action e2e gets
all the way to the last step and dies there — both C and C++:

```
Client: Goal accepted by server, waiting for result
Client: Next number in sequence received: [0] … [0, 1, 1, 2, 3, 5]   # feedback OK
Client: Failed to deserialize result                                  # get-result reply
Server: Received goal request … Publish feedback ×11 … Goal succeeded
```

`test_rtos_action_e2e::platform_1_Platform__Freertos::lang_{2_C,3_Cpp}` fail
with `accepted=true, completed=false`. Pubsub + service e2e on the identical
images/runtime are GREEN (4/6), so session, pub/sub, and request/reply all
work — only the action get-result reply payload fails to decode client-side.

## Not a migration regression

The pre-287 freertos C/C++ role images baked `tcp/10.0.2.2:7447` while the
rtos_e2e harness listens per-(variant,lang) (`7551`–`7671`) on a
`192.0.3.0/24` slirp — those lanes could never even connect, so this is the
FIRST time the freertos C/C++ action path has actually been exercised
end-to-end by rtos_e2e. The migration exposed the bug; it did not cause it.

## Evidence / already ruled out

- Generated `Fibonacci_*` bindings carry `int32_t data[64]` — an order-10 goal
  (11 elements) is nowhere near capacity.
- Feedback frames with the same `sequence` field deserialize fine right up to
  the result, so the type/serializer basics are sound.
- Native comparison blocked in-session (workspace action fixtures stale);
  the native action path is covered by `c_action_roundtrip_xprocess_e2e` and
  was green pre-287.
- Smells like the XRCE double-CDR-header class (archived: the XRCE action
  feedback trampoline double-framed payloads) but on the zenoh get-result
  REPLY path — the reply travels zenoh query/reply, not pub/sub, so a framing
  mismatch would surface exactly here and nowhere else.

## Repro

```
just freertos build-fixture-extras
cargo nextest run -p nros-tests --test rtos_e2e \
  -E 'test(~Freertos) and test(~action) and test(~Lang__C)'
```

## Next steps

- Dump the raw get-result reply bytes client-side (zpico shim log) and diff
  against the native reply framing (extra CDR header? status-wrapper offset?).
- Check `try_handle_get_result` / reply-path buffering on the freertos zpico
  shim (`Z_FEATURE_QUERY` reply buffer limits; issue-0135's config-mismatch
  class is gated but worth re-verifying on this image).

## Scope update (same session)

nuttx C/C++ action e2e fail the same way (serialized run; nuttx pubsub 3/3 and
service rust+C green on the same images), and nuttx **cpp service** also fails
— triage of whether that is this reply-path bug or a separate cpp issue is
pending.

threadx-linux C/C++ fail with the IDENTICAL signature (all 11 feedback frames
deliver, then `Failed to deserialize result`; `accepted=true, completed=false`)
on its 4/6-green lane — so this is NOT freertos-specific: the bug lives in the
shared rmw-zenoh-cffi get-result reply path (or the nros-c client-side result
decode), exercised the same way by both platforms' native-identical images.

## Root-cause scoping (bisect-by-baseline, same day)

- **NATIVE reproduces**: `native_api::test_c_action_communication` fails with
  the identical signature on freshly built fixtures.
- **Pre-existing on trunk, NOT a phase-287 regression**: a worktree at
  `0d9484b20` (pre-session tip, untouched sources) with freshly rebuilt
  native fixtures fails IDENTICALLY — the lane only ever looked green on
  stale museum binaries (the 0148/0164 treadmill class).
- **zenoh-only**: `c_xrce_api::test_c_xrce_action_fibonacci` (same portable
  main.c, XRCE backend) passes — the bug lives in the zenoh rmw get-result
  query/reply path, not in the examples, the action layer API, or CDR
  generally. Feedback frames on the SAME payload type deserialize fine and
  the client sometimes drops/coalesces feedback frames before dying —
  suggests reply-buffer handling in the zenoh cffi reply path.
