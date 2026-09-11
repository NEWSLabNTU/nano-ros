// phase-454 W6.c — RFC-0100 D11: the derived heap floor and the boot check.
//
// The predicate `session_create` runs before it allocates anything, exercised
// across values rather than at the single point this TU's own compile line fixes.
//
// What this test is FOR: the check has to be able to say "too small" AND it has
// to be unable to say it wrongly. A floor that is never short is a green light
// that means nothing (the `check-reconfigure-stale` lesson), and a floor that is
// short for a working image is worse than no check at all, because it refuses a
// build that would have run.

#include <stdio.h>

// Pin the image-shaped constants so the assertions below are about the FORMULA
// and not about whatever the enclosing build happened to derive. Both are
// `#ifndef`-guarded in the header, which is what makes this possible.
#define NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES 8
#define NROS_CYCLONEDDS_MAX_KINDS 16

#include "heap_budget.hpp"

using namespace nros_rmw_cyclonedds;

static int failures = 0;

static void check(bool ok, const char* what) {
    if (!ok) {
        fprintf(stderr, "FAIL: %s\n", what);
        ++failures;
    }
}

int main() {
    // --- the floor is made of terms that are all certain ------------------

    // The baked `<Sizing>` receive buffers alone, with nothing registered.
    check(required_heap_bytes(0, 0) == kReceiveBufferBytes + kReceiveBufferChunkBytes,
          "an image registering no type still needs the receive buffers");

    // Every registered type costs a descriptor and a name. Monotonic in the
    // count -- if this ever stopped holding, a bigger workspace would ask for
    // LESS heap, which is the direction that ships an under-sized image.
    check(required_heap_bytes(1, 0) > required_heap_bytes(0, 0), "one more type costs more heap");
    check(required_heap_bytes(100, 0) > required_heap_bytes(10, 0),
          "the per-type term scales with the count");

    // And the largest schema's ops array is charged once, so `max_kinds` moves
    // it too. This is the term that makes `[types].max_kinds` load-bearing
    // rather than decorative.
    check(required_heap_bytes(4, 64) > required_heap_bytes(4, 8),
          "a deeper/wider largest schema costs more heap");

    // Exact arithmetic at one point, so a refactor that quietly drops a term
    // fails rather than staying plausible.
    check(required_heap_bytes(0, 1) == kReceiveBufferBytes + kReceiveBufferChunkBytes +
                                           kMinOpsWordsPerKind * sizeof(uint32_t),
          "one kind costs exactly its minimum ops words");

    // --- the predicate: three states, not two -----------------------------

    const size_t need = required_heap_bytes(8, 16);

    // NOT STATED is never short. "Nobody said" and "too small" are different
    // facts, and a board that states no heap must not be judged (RFC-0100 D6).
    check(!budget_is_short(/*stated=*/false, 0, 8, 16),
          "an unstated budget is never short, not even at zero");
    check(!budget_is_short(false, 1, 8, 16), "an unstated budget is never short");

    // THE NEGATIVE CONTROL. A stated budget under the floor IS short -- without
    // this the check could be a function that always returns false and every
    // assertion above would still pass.
    check(budget_is_short(true, need - 1, 8, 16), "a budget one byte short is short");
    check(budget_is_short(true, 0, 8, 16), "a zero budget is short");
    check(budget_is_short(true, 16u * 1024u, 8, 16),
          "16 KiB -- a Zephyr malloc arena at its default -- is short for Cyclone");

    // And it is not short at or above the floor: the check must never refuse an
    // image that would have worked.
    check(!budget_is_short(true, need, 8, 16), "exactly the floor is enough");
    check(!budget_is_short(true, need + 1, 8, 16), "above the floor is enough");
    check(!budget_is_short(true, 4u * 1024u * 1024u, 8, 16), "4 MiB is plenty");

    // --- this TU's own image constants ------------------------------------
    //
    // No budget is defined here, so the image-level predicate is silent. That
    // is the state every existing image is in until a board states
    // `[board.knobs.memory] heap_bytes`, and it must stay a no-op.
    check(!kHeapBudgetStated, "this TU states no budget");
    check(!heap_budget_is_short(), "an image with no stated budget is never refused");
    check(kRequiredHeapBytes == need, "the image constant is the formula at its own knobs");

    if (failures != 0) {
        fprintf(stderr, "heap_budget_check: %d failure(s)\n", failures);
        return 1;
    }
    printf("heap_budget_check: OK (floor %zu bytes at 8 types / 16 kinds)\n", need);
    return 0;
}
