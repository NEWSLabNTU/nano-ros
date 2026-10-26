---
id: 2089
title: "The generated typed C service path has no request-id flavour"
status: open
type: limitation
area: [api, codegen]
severity: low
found: 2026-10-26
related: [issue-2081, phase-515]
---

**Home: [phase-515](../roadmap/phase-515-api-stock-shape-review.md)** (R7). This is residue from issue 2081.

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
