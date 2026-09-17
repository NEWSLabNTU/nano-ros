/*
 * phase-417 W5.e — NEGATIVE CONTROL. This file MUST FAIL to compile.
 *
 * W5.a's subscription twin exists because `nros_executor_add_subscription_typed`
 * takes the caller's storage as `void *`: the wrong struct registered silently
 * and was then overwritten with a message of another type on every dispatch,
 * with no allocator anywhere to notice. The generated macro had to reintroduce
 * the type (`1 ? (msg) : (<Msg>*)0`) before a compiler would say anything.
 *
 * The service path does NOT have that hole — the handler block and the handler
 * function are both generated TYPES, so a mismatch is already ill-typed. That
 * is a claim, and this file is what measures it. A service has TWO payloads
 * going in opposite directions, so the mistake it invites is not "the wrong
 * message" but "the two swapped", which a `void (*)(const void *, void *,
 * void *)` — rclc's own callback type — could not diagnose at all.
 *
 * Both mistakes below are diagnosed by C as WARNINGS, not errors (the same
 * asymmetry `rcl_node_init_reorder_probe` records), so `check c` compiles this
 * with -Werror and REQUIRES a non-zero exit. The positive control
 * `typed_service_handler.c` compiles the same entry points with the arguments
 * the right way round, so a guard that rejected everything would be caught
 * there rather than read here as rigour.
 *
 *   cc -fsyntax-only -std=c11 -Wall -Wextra -Werror \
 *       -Ipackages/cli/rosidl-codegen/tests/fixtures/fingerprint-corpus/expected/configured \
 *       -Itarget/nros-c-generated -Ipackages/api/nros-c/include \
 *       -Ipackages/platform/nros-platform-api/include \
 *       packages/api/nros-c/tests/compile/typed_service_storage_mismatch_probe.c
 */

#include "Probe.srv.h"

#include "nros/client.h"
#include "nros/service.h"

/* ── Mistake 1: the two payloads swapped ──────────────────────────────────
 *
 * The request goes IN and the response comes OUT, so the handler's two
 * pointers are not interchangeable — and they are the same "shape" to a
 * reader. Written against `void *` this compiles and then decodes a request
 * into response storage. */
static void swapped_payload_handler(const fingerprint_corpus_srv_probe_response* request,
                                    fingerprint_corpus_srv_probe_request* response, void* context) {
    (void)request;
    (void)response;
    (void)context;
}

static fingerprint_corpus_srv_probe_service_handler_t g_handler;

static nros_ret_t install_swapped_handler(void) {
    /* Expected: incompatible pointer type — the parameter is a
     * `fingerprint_corpus_srv_probe_handler_fn_t`, and the compiler names both
     * function types. */
    return fingerprint_corpus_srv_probe_service_handler_init(&g_handler, swapped_payload_handler,
                                                             NULL);
}

/* ── Mistake 2: the client's storage block on the server seam ──────────────
 *
 * `..._client_handler_t` and `..._service_handler_t` differ by exactly the
 * request storage the server needs, so the server trampoline would deserialize
 * a request off the end of the block. */
static fingerprint_corpus_srv_probe_client_handler_t g_wrong_block;
static struct nros_service_t g_service;

static nros_ret_t bring_up_with_client_block(const struct nros_node_t* node) {
    /* Expected: incompatible pointer type, naming both handler structs. */
    return fingerprint_corpus_srv_probe_service_init(&g_service, node, "/add", &g_wrong_block);
}

const void* nros_typed_service_mismatch_anchors[] = {
    (const void*)(const void* const*)&install_swapped_handler,
    (const void*)(const void* const*)&bring_up_with_client_block,
};
