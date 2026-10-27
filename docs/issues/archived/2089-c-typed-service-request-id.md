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

Resolved by phase-515 C3 (`86ab899b52`), in rclc's shape rather than the
per-service shape proposed above. `rclc_executor_add_service_with_request_id(
executor, service, request_msg, response_msg, callback)` takes rclc's own
`rclc_service_callback_with_request_id_t`
(`void (*)(const void *, rmw_request_id_t *, void *)`). The executor
deserialises each request into the caller's message through the descriptor's
codecs, which `rcl_service_t.type_support` now carries, and serialises the
caller's response back. A ported rclc program compiles unchanged, which a
generated `<Srv>__service_handler_init_with_request_id` would not have given:
no rclc program spells that name. So codegen needs no template change and
gains no request-id flavour.

Test: `packages/api/nros-c/tests/run/typed_service_client.c` (`just check c`)
stages a request with a known writer GUID and sequence number and asserts the
callback sees them. Ledger: `c:executor_add_service_with_request_id` is
`adopt-bounded`; `c:service_callback_with_request_id_t` is the callback type.

An earlier fix for this issue (PR #2746) took the same approach and was closed
unmerged once C3 had landed it.
