/* Issue 1612 — a too-small take names the size the sample needed.
 *
 * `rmw_vtable.h` asks the subscription `take` slot to leave the refused
 * sample's size in the span's `len` on `NROS_RMW_RET_BUFFER_TOO_SMALL`. Before
 * it, the refusal said only THAT the sample did not fit, and the C++ drop log
 * could name the buffer but never the sample.
 *
 * NO AGENT: the ring is filled directly, the way the topic callback fills it,
 * because what is under test is what the TAKE reports about an entry it cannot
 * hand over -- not how the entry arrived. Same construction as
 * `entity_lifetime.c`.
 */

#include "internal.h"
#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int failures = 0;

#define CHECK(cond, what)                                                                          \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            printf("  FAIL: %s\n", (what));                                                        \
            failures++;                                                                            \
        } else {                                                                                   \
            printf("  ok    %s\n", (what));                                                        \
        }                                                                                          \
    } while (0)

/* Push one staged sample of `len` bytes, as the topic callback would. */
static void stage(xrce_subscriber_slot* slot, size_t len) {
    xrce_subscriber_ring_entry* entry = &slot->entries[slot->write_idx];
    memset(entry->data, 0xab, len);
    entry->len = len;
    entry->overflow = false;
    slot->write_idx = (uint16_t)((slot->write_idx + 1) % XRCE_SUBSCRIBER_RING_DEPTH);
    slot->count++;
}

int main(void) {
    printf("issue 1612: a too-small XRCE take reports the sample's size\n");

    /* The sample must fit the backend's own staging buffer (or it is a
     * MESSAGE_TOO_LARGE, a different refusal) and overrun the caller's. */
    const size_t sample = XRCE_SUBSCRIBER_BUFFER_SIZE < 300 ? XRCE_SUBSCRIBER_BUFFER_SIZE : 300;
    const size_t cap = sample / 2;

    xrce_subscriber_slot* slot = (xrce_subscriber_slot*)calloc(1, sizeof(xrce_subscriber_slot));
    if (slot == NULL) {
        printf("  FAIL: could not allocate a subscriber slot\n");
        return 1;
    }
    slot->active = true;
    xrce_subscriber_state state;
    memset(&state, 0, sizeof(state));
    state.slot = slot;
    rmw_subscription_t sub;
    memset(&sub, 0, sizeof(sub));
    sub.backend_data = &state;

    uint8_t buf[XRCE_SUBSCRIBER_BUFFER_SIZE];

    /* 1 — refused: BUFFER_TOO_SMALL, and `len` is the size it needed. */
    stage(slot, sample);
    rmw_mut_byte_span_t span = {buf, cap, NROS_RMW_TAKE_LEN_UNKNOWN};
    /* `taken` is not read on a failure: `rmw_vtable.h` writes the
     * out-parameters only on OK. */
    bool taken = false;
    rmw_ret_t rc = xrce_subscription_take(&sub, &span, &taken);
    CHECK(rc == NROS_RMW_RET_BUFFER_TOO_SMALL, "a sample over the buffer is BUFFER_TOO_SMALL");
    CHECK(span.len == sample, "and `len` names the size the sample needed");
    printf("        sample %zu bytes, buffer %zu, reported len %zu\n", sample, cap, span.len);
    CHECK(slot->count == 0, "the refused sample is consumed, so the ring cannot wedge");

    /* 2 — the same sample into a buffer that holds it: the ordinary take. */
    stage(slot, sample);
    span.capacity = sizeof(buf);
    span.len = NROS_RMW_TAKE_LEN_UNKNOWN;
    rc = xrce_subscription_take(&sub, &span, &taken);
    CHECK(rc == NROS_RMW_RET_OK && taken && span.len == sample,
          "a buffer that fits takes the sample, same length");

    free(slot);
    if (failures != 0) {
        printf("%d check(s) failed\n", failures);
        return 1;
    }
    printf("all checks passed\n");
    return 0;
}
