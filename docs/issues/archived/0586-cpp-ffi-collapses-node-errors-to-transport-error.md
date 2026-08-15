---
id: 586
title: "The C++ FFI discards 15 backend errors and returns `-100 TRANSPORT_ERROR`
  for causes that are not transport"
status: resolved
type: tech-debt
area: api-cpp
related: [issue-0436, issue-0557, issue-0428, phase-358]
---

## Symptom

`NROS_CPP_RET_TRANSPORT_ERROR` (`-100`) is documented in its own source as "the
catch-all for unmapped variants". Fifteen call sites in the C++ FFI reach it by
throwing the real error away:

```
$ grep -rn 'Err(_) => NROS_CPP_RET_TRANSPORT_ERROR' packages/api/nros-cpp/src/
  packages/api/nros-cpp/src/publisher.rs      3
  packages/api/nros-cpp/src/subscription.rs   5
  packages/api/nros-cpp/src/service.rs        7
```

A C or C++ caller therefore sees "transport error" for a name that was too long,
a buffer that was too small, a slot that was already in flight, or a backend that
does not implement the operation. On an embedded guest, where the return code is
often the ONLY thing that reaches the console, that is the difference between a
diagnosis and a guess.

## Why this is filed rather than fixed

Issue 0557 hit one of these and fixed exactly one — `nros_cpp_action_server_create`,
whose fallible call returns a `NodeError` and so maps cleanly through the existing
`node_error_to_cpp_ret` (which additionally names the variant on the error path,
issue 0436's fix for `nros_cpp_init`).

The remaining fifteen do NOT share one error type. Their fallible calls are
`create_publisher`, `commit_slot`, `create_subscription`, `create_service`,
`create_client`, `send_request_raw`, `try_recv_reply_raw` and the executor
`register_*` family. Some return `NodeError`, some `TransportError`, some a
backend-specific error. Rewriting them all to one mapper without reading each
signature would replace a wrong-but-uniform answer with a wrong-and-varied one.

## What a fix looks like

* one mapper per error type, siblings of `node_error_to_cpp_ret`, each
  documenting which `NROS_CPP_RET_*` a variant lands on and why;
* the same `eprintln!("nros: …{err:?}")` on the error path that 0436 added — it
  costs nothing on a path that has already failed and is often the only way to
  see the cause on a guest;
* a check that `-100` is returned only where the cause really is transport, so
  the constant means what its name says.

## Acceptance

* no `Err(_)` discards a backend error in `packages/api/nros-cpp/src/`;
* each remaining `NROS_CPP_RET_TRANSPORT_ERROR` return is reachable only from a
  transport cause;
* a gate, so the sixteenth site cannot be added silently.

## Not to be confused with

Issue 0436 fixed this for `nros_cpp_init` and named the class in its comment;
issue 0557 is the case that showed the class was still live everywhere else.


## RESOLVED 2026-08-15

All fifteen sites map their error now. The types came from the compiler rather
than from reading each call chain: replacing every `Err(_)` with
`Err(e) => { let _: () = e; … }` and compiling makes rustc name all fifteen at
once, including the four behind `lending` / `safety-e2e` features that a default
build never reaches.

    NodeError       (5)  service.rs 258, 622; subscription.rs 243, 343, 458
    TransportError (10)  publisher.rs 124, 255, 282; service.rs 146, 452, 675,
                         697, 734; subscription.rs 122, 694

`NodeError` sites go through the existing `node_error_to_cpp_ret`.
`TransportError` sites go through a new sibling, `transport_error_to_cpp_ret`,
which also replaces the inline `match` that `node_error_to_cpp_ret` used to carry
for its `Transport(t)` arm — so there is ONE mapping per error type, in one
place.

Both mappers are exhaustive: no `_` arm, which rustc enforces by rejecting an
unreachable one. Adding a variant to either enum now fails to compile until
someone decides what a C++ caller should see, instead of silently joining the
`-100` pile. That is the property worth having; the fifteen call sites were only
the symptom.

`TRANSPORT_ERROR` is kept for what it names: connection, peer, wire, and a
backend-specific string this layer cannot interpret. Entity-creation failures map
to `PUBLISH_FAILED` / `SUBSCRIPTION_FAILED` / `SERVICE_FAILED`, capacity to
`FULL`, `WouldBlock`/`NoData` to `TRY_AGAIN`, `IncompatibleQos` to `NOT_ALLOWED`,
`Unsupported`/`LoanNotSupported` to `UNSUPPORTED`, and so on.

### Gate

`scripts/check-cpp-ffi-error-mapping.py`, wired into the fast checks. It rejects
`Err(_) => …TRANSPORT_ERROR` and a `_` arm in either mapper. Self-tested in both
directions before landing.

Scope was corrected during that self-test. The first version matched every
`Err(_) => <any code>` and reported 43 sites — most of them correct, e.g.
`Err(_) => NROS_CPP_RET_FULL` on a timer arena, where "full" is the whole truth
and the error carries nothing more. A gate that flags correct code teaches people
to ignore it. It now matches only the discard into the catch-all, which is the
one return value that asserts "we did not look".
