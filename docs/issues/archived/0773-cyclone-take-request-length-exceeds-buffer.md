---
id: 773
title: "Cyclone service/action e2e red on main — `take_request` reports a
  length larger than the buffer it was given, and the CFFI shim slices on it
  (`range end index 1005 out of range for slice of length 256`)"
status: resolved
type: bug
area: rmw, cyclonedds
related: [phase-376]
---

## Symptom

Six real failures in a tier-1 sweep on newslab-241 (2026-08-24), all
Cyclone DDS request/reply:

```
nros-tests::native_api  test_native_cyclonedds_rust_action
nros-tests::native_api  test_native_cyclonedds_service_callback::lang_2_Language__Cpp
nros-tests::native_example_reqresp_e2e  case_06_cpp_cyclone_service
nros-tests::native_example_reqresp_e2e  case_13_rust_cyclone_action
nros-tests::native_example_reqresp_e2e  case_14_c_cyclone_action
nros-tests::native_example_reqresp_e2e  case_15_cpp_cyclone_action
```

The action one panics with the decisive line:

```
thread 'main' panicked at packages/rmw/cffi/src/lib.rs:2755:23:
range end index 1005 out of range for slice of length 256
```

The five others report the softer shape of the same thing — "client
never logged the server-computed result", i.e. the round-trip did not
complete.

## What that line is

`packages/rmw/cffi/src/lib.rs` (`take_request`):

```rust
let rc = unsafe { (self.vtable.take_request…)(&mut view, buf.as_mut_ptr(), buf.len(),
                                              &mut seq, &mut out_len, &mut taken) };
…
let len = out_len;
Ok(Some(ServiceRequest { data: &buf[..len], … }))     // 2755
```

So the backend returned `NROS_RMW_RET_OK`, `taken = true`, and
`out_len = 1005` against a 256-byte buffer, and the shim sliced on that
number without checking it against the capacity it just passed in.

Two defects, and they are separable:

1. **Shim (`nros-rmw-cffi`) trusts a backend-reported length.** A
   backend that reports `out_len > buf.len()` should be a loud
   `TransportError`, not a slice panic in the middle of a user's
   executor. Every `take_*` slot that returns a length has this shape.
2. **Cyclone reports it.** `service_take_request`
   (`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/service.cpp:903`)
   forwards `service_try_recv_request_len`'s value straight into
   `*out_len`. The length-producing path via `split_wire_header` DOES
   guard (`if (user_len > user_cap) return NROS_RMW_RET_BUFFER_TOO_SMALL`),
   so either the action path reaches a different producer, or a
   negative/oversized return is surviving the `int32_t` → `size_t`
   conversion. 1005 bytes is a plausible action-goal wire length, which
   points at "the guard is not on this path" rather than at a corrupt
   value.

## Not caused by the play_launch/rlm pin advance

The failure set is byte-identical to a tier-1 run taken BEFORE that
change on the same host (and is one member smaller after it — the
custom-transport flake did not recur). One member
(`case_06_cpp_cyclone_service`) was separately re-run with the working
tree stashed, i.e. on clean `main`, and failed identically.

## Suspicion

`phase-376 W3.b/W3.d` renamed `try_recv_request`/`try_recv_reply` to
`take_request`/`take_response` (commit `c5759a397`) and W4 grew the
vtable by ~30 slots. The C++ vtable is a POSITIONAL initializer with
`/*slot*/` comments — a comment can agree with the reader while the
position no longer agrees with the header. Worth eliminating first,
because it explains "OK + taken + nonsense length" better than any
arithmetic bug does.

## Direction

* Shim: reject `out_len > buf.len()` at every take slot with a named
  error. Cheap, and it converts a panic into a diagnosis.
* Cyclone: find the action/service path that produces a length without
  the `user_cap` guard.
* Consider a debug-build assertion in the vtable assembly that the
  positional initializer's comment names match the header's field order
  — the class this whole surface is exposed to.

## Evidence 2026-08-24 — the length is REAL, so the vtable suspicion does not hold

Measured, not reasoned:

* **Raising `CANCEL_BUF` 256 -> 1024 makes `test_native_cyclonedds_rust_action`
  pass END TO END in 4.9 s.** A misaligned positional slot would have produced a
  valid slice over GARBAGE — the action would not have completed. So the take
  returns a real 1005-byte payload, and the "nonsense length" reading is out.
* **`CANCEL_BUF` is the outlier, not the norm.** `GOAL_BUF`, `RESULT_BUF` and
  `FEEDBACK_BUF` all derive from `DEFAULT_RX_BUF_SIZE` (1024); cancel alone was a
  hardcoded 256. 1005 fits 1024 and not 256, which is exactly why only the cancel
  path crashed.
* **Nothing is being stolen.** With the larger buffer the CLIENT still receives
  its result, so the cancel reader is getting a COPY of a message rather than
  consuming one the client needed. In DDS that means a reader matched to a topic
  it should not be matched to.

Ruled out by reading, so the next person need not repeat it:

* Service topic naming (`service_topic_name`, `service.cpp:237`) builds
  `rq/<service>Request` / `rr/<service>Reply`; `cancel_goal` and `get_result`
  cannot collide by name.
* The action keys are distinct (`cancel_goal_key` / `get_result_key`,
  `nros-rmw/src/traits.rs:158-166`).
* `cancel_buffer` has exactly ONE take site
  (`action_core.rs:529`, `try_recv_cancel_request`), so the 256-byte slice in
  the panic is unambiguously the cancel path.

**Still unexplained, and it is the actual defect:** why a *cancel* request take
yields ~1005 bytes at all. `CancelGoal_Request` is a goal UUID and a stamp. The
next step is runtime observation — dump the first bytes and the matched topic at
the take when `out_len > buf.len()` — not more reading; the static explanations
are exhausted.

**Do not fix this by enlarging the buffer alone.** `ActionServerCore` crosses the
C ABI as a probe-sized `_opaque` array, so +768 bytes lands in every C/C++ image
that instantiates an action server (`ACTION_SERVER_OPAQUE_U64S` catches it at
compile time — issue 0472's mirror class). That footprint is only justified once
the 1005 is explained.

**Adjacent finding:** `CANCEL_BUF` governs only the struct field. Four
constructors spell `[0u8; 256]` as a literal (`action.rs:279,693`,
`node.rs:792,1081`), so the constant never actually sized the buffer — changing
it alone is a type error, and a literal that happened to match would be a silent
no-op.

The shim half of this issue's Direction landed in `b58eb11e0`: `checked_take_len`
rejects `out_len > buf.len()` at all three copying-take sites, so the six cells
now fail at their own assertions instead of killing the server process.

## Root cause + fix (2026-08-24) — the "length" was a RETURN CODE

Measured with a probe on the take: the oversize hits carry
`matched publications: 0` (nothing is even connected), the scratch
buffer is untouched (all-zero on the first hit, stack pointers later),
and `[0773t]` — a probe on `take_typed_wire`'s branch decision — never
fires for the cancel reader at all. So no sample was ever taken.

The numbers name themselves:

```
NROS_RMW_RET_EXTENSION_BASE   1000
NROS_RMW_RET_NO_DATA          1003     <- the reported "wire length"
NROS_RMW_RET_BUFFER_TOO_SMALL 1005     <- the reported "payload length"
```

Phase-376 W3.d step B renumbered `nros_rmw_ret_t` to upstream rmw's
values, which are all **non-negative** (`ERROR` = 1,
`INVALID_ARGUMENT` = 11, extensions 1000+). Six helpers in the Cyclone
backend return "byte count **or** status" in one `int32_t`, and every
consumer tested `< 0` / `<= 0` for the status — a test that stopped
firing the moment the codes went positive. An EMPTY queue therefore
read as a 1003-byte sample: `taken = true`, `out_len = 1003`, and the
shim sliced `1005` out of a 256-byte buffer.

That retires both earlier readings recorded above:

* The length was never real, so nothing was being "copied from a
  reader matched to a topic it should not be matched to" — there was
  no writer at all on the first hit.
* Raising `CANCEL_BUF` 256 -> 1024 "passed" only because 1003 then
  FITS: the code would have deserialized an empty scratch buffer as a
  cancel request nobody sent. The `ActionServerCore` footprint that
  change would have cost every C/C++ image is not needed.

**Fix (the class, not the site).** `service.cpp` gains one explicit
encoding — `wire_status()` / `wire_is_status()` / `wire_status_code()`
— so "is this a status?" is a property of the value rather than a
coincidence of the numbering; all 19 status returns inside the six
length-returning helpers travel negated, and every consumer uses the
named predicate. Two adjacent defects fell out on the way:

* the `take_request` / `take_response` vtable adapters leaked
  `WOULD_BLOCK` as a failure; both now report `taken = false` + OK,
  matching the shim's contract;
* `subscription_take_sequence_count` (`subscriber.cpp`) had the
  identical defect one TU over — an `ERROR` (= 1) would have been read
  as "1 message taken".

**Verified** on the host where these were deterministic reds: 7/7 pass
(`test_native_cyclonedds_rust_action`,
`test_native_cyclonedds_service_callback` C + C++,
`case_06_cpp_cyclone_service`, `case_13/14/15` cyclone actions), and a
manual action run completes end to end
(`Result received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]`).

**Worth a gate.** The hazard is generic: any function returning
"count-or-status" in one signed integer is now unsafe by default in
this tree. A checker for `int32_t`-returning functions that `return
NROS_RMW_RET_*` un-negated would catch the next one — noted for
phase-376's own follow-up rather than added here.

## The gate, and the two instances it found (follow-up, 2026-08-24)

The section above notes the hazard is generic and defers the checker. Added
now, in `scripts/check-rmw-ret-sign.py`, and wired to `just check` as
`check-rmw-ret-sign`.

That script reported **0 / 0 throughout this bug**. It scans a WINDOW around
vtable slot names, and every helper here is internal — no slot name appears
within thirty lines of some of the tests. The window cannot be widened into
this, because the defect is not "a sign test near a slot"; it is "a function
that returns a length OR a status at all". So the new check is structural: a
C/C++ function whose body returns both a bare `NROS_RMW_RET_*` constant and a
cast-to-integer length, wherever it sits.

It does NOT fire on the `wire_status()` encoding this issue landed — a negated
status is not a bare status return, which is the point of that encoding.

Two more instances, found the moment it ran:

* `call_blocking` in `tests/service_roundtrip.cpp` and
  `tests/ros2_srv_client.cpp` — cyclonedds' own test helpers, carrying the bug
  they exist to catch. Converted to status-plus-out-parameter.

And one in another backend, which this issue's fix did not reach:

* `xrce_service_try_recv_request_len` / `_reply_raw_len` (`xrce/service.c`)
  carried the IDENTICAL W3.d step A "thin adapter" comment and the identical
  `if (n < 0)`. xrce had simply never been exercised on a failing path. Both
  converted to status-plus-out-parameter (rather than the negated encoding —
  the xrce helpers have no caller that needs the packed form).

**A hole in the gate, found by probing it rather than trusting it:** the first
version matched only `intN_t` function heads. `rmw_ret_t` is a typedef for
`int32_t`, so the same defect spelled with the status type passed straight
through. Widened, then re-probed by reintroducing the original bug and
confirming a red.
