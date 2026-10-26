---
id: 2089
title: "The generated typed C service path has no request-id flavour"
status: resolved
type: limitation
area: [api, codegen]
severity: low
found: 2026-10-26
resolved: 2026-10-27
related: [issue-2081, phase-515]
---

**Home: [phase-515](../../roadmap/phase-515-api-stock-shape-review.md)** (R7). This is residue from issue 2081.

## What

Issue 2081 gave C a request-id service callback at the RAW level:
`rclc_executor_add_service_raw_with_request_id`. Its callback takes
`(request, len, rmw_request_id_t *request_id, response, cap, *len, ctx)`,
which is rclc's `rclc_service_callback_with_request_id_t` argument order in
CDR-byte form.

The TYPED per-service path that codegen emits
(`packs/c/service.h.jinja`: `<Srv>__service_handler_init(&handler, cb, ctx)` /
`<Srv>__executor_add_service`) has no request-id variant. A C program that
wants the typed request and response structs and also the requester's id
must drop to the raw entry point and do CDR by hand.

## Possible shape

`<Srv>__service_handler_init_with_request_id(&handler, cb, ctx)`, with
`cb(const <Srv>_Request *, rmw_request_id_t *, <Srv>_Response *, void *ctx)`.
The generated trampoline would register through
`rclc_executor_add_service_raw_with_request_id`. This is a codegen template
change, so it needs a codegen version bump and regenerated goldens.
`c:executor_add_service_with_request_id` states the gap.

## Resolution (2026-10-27)

Resolved with rclc's own entry point, not a per-service codegen helper, so a
ported rclc program compiles unchanged:

```c
typedef void (*rclc_service_callback_with_request_id_t)(
    const void *request, rmw_request_id_t *request_id, void *response);
rcl_ret_t rclc_executor_add_service_with_request_id(
    rclc_executor_t *executor, rcl_service_t *service,
    void *request_msg, void *response_msg,
    rclc_service_callback_with_request_id_t callback);
```

Same name, arity and callback type as rclc. The "possible shape" above
(`<Srv>__service_handler_init_with_request_id`) would have given typed C a
request id under a name no rclc program uses. rclc's typed form is generic over
the service descriptor's codecs, and the descriptor has carried them since
phase-492 W8. So no template change was needed, exactly as for
`rclc_executor_add_subscription_with_context`.

**How.**
- `rcl_service_t` gained a trailing `type_support`: the descriptor
  `rclc_service_init_*` was given. The registration reads the request
  deserialiser and response serialiser from it, as a subscription reads its
  descriptor. A descriptor with no codecs is refused with `RCL_RET_UNSUPPORTED`.
- `nros-node`: a typed dispatch for the raw service entry
  (`TypedServiceRequestIdDispatch`) carries the two caller-owned structs, the
  codecs and the context-less callback. Nothing is kept in a wrapper. Public
  entry `Executor::register_service_c_typed_with_request_id_sized`.
- rclc's callback returns `void`, so the executor refuses on its behalf. A
  request that does not decode never reaches the callback. A response that does
  not encode is not sent. Both are logged, rate-limited. The id is issue 2081's
  (`take_request_with_info`), all zero when not reported.
- `NROS_CODEGEN_VERSION` 20 -> 21, MIN unchanged. The surface gate hashes
  `rcl_service_t`'s text. Generated C names the struct only through a pointer,
  so a version-20 tree still compiles. Goldens, surface baseline and the
  buildless config snapshots were regenerated.

**Test.** `packages/api/nros-c/tests/run/service_request_id_typed.c`, in
`just check c`, on the stub backend with hand-written codecs. It covers: the
deserialised request plus the client's GUID and sequence number; the response
sent against the backend's token; an all-zero id when none is reported; no
callback and no reply for an undecodable request; no reply for an unencodable
response; `RCL_RET_UNSUPPORTED` / `RCL_RET_INVALID_ARGUMENT` refusals. Negative
control, measured: building the id from `RawRequestId::default()` in the
dispatch fails the GUID and sequence checks.

**Ledger.** The `c:executor_add_service_with_request_id` row is deleted: the
report now finds the item the SAME as rclc's. New rows:
`c:service_callback_with_request_id_t` and
`rust:Executor::register_service_c_typed_with_request_id_sized`. The
`c:executor_add_service_raw_with_request_id` and `c:take_request_with_info`
rows were re-read.

**Residue.** None filed. The same dispatch would carry rclc's plain
`rclc_executor_add_service` and `_with_context`, which `rcl_compat.h` still
refuses on the two grounds this issue answered. Adopting those is already
planned as phase-515 C3.
