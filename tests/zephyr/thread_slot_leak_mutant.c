/*
 * phase-460 W6 -- the leaking shim the gate must catch.
 *
 * This is NOT part of the product. It is the mutant half of the gate: the
 * runner builds the shims a second time with
 * `-Dnros_zephyr_task_slot_release=<renamed>`, so the real release is compiled
 * under another name and THIS definition -- which forgets to give the slot
 * back -- is what the test links against. The `n-plus-2` case must then FAIL.
 *
 * Without this, a green test says only "the assertions ran". It could be green
 * because the pool never fills, because the capacity macro does not reach the
 * shims, or because the case returns before it asserts anything. The mutant
 * makes the gate's teeth a measurement rather than a claim, which is what the
 * fix in issue 0839 has been missing since it landed.
 *
 * The leak this reproduces is the pre-0839 shim exactly: a claim with no
 * matching release, so the pool drains one slot per join and the image walks
 * out of threads at its third reconnect.
 */

#include <pthread.h>

#include "thread_slot_shim_decls.h"

void nros_zephyr_task_slot_release(pthread_t owner) {
    (void)owner;
    /* Deliberately nothing. */
}
