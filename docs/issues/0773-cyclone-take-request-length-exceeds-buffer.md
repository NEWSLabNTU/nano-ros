---
id: 773
title: "Cyclone service/action e2e red on main — `take_request` reports a
  length larger than the buffer it was given, and the CFFI shim slices on it
  (`range end index 1005 out of range for slice of length 256`)"
status: open
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
