/* The stub board hook: HOOK_ANSWERS 1 answers a fixed instant, 0 never
 * answers, 2 answers only from 200 ms after its first call (a board whose
 * clock syncs late), 3 answers only once the test has set
 * `hook_network_up` (a board clock behind an SNTP exchange, which cannot
 * answer before the network is up), 4 answers the instant on its first call
 * and the instant + 1 h on every later one (a board clock its own time service
 * stepped after boot: only a re-ask can see the step). An object of the
 * image, as the link rule asks. */
#include <stdbool.h>
#include <stdint.h>
#include <time.h>

#define BOARD_NS 1700000000123456789ULL /* 2023-11-14 */
#define HOOK_AFTER_MS 200
#define BOARD_STEP_NS 3600000000000ULL /* 1 h */

int hook_calls;
volatile int hook_network_up;

__attribute__((unused)) static uint64_t mono_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

bool nros_board_wall_clock_ns(uint64_t* out_ns);
bool nros_board_wall_clock_ns(uint64_t* out_ns) {
    hook_calls++;
#if HOOK_ANSWERS == 1
    *out_ns = BOARD_NS;
    return true;
#elif HOOK_ANSWERS == 2
    static uint64_t first_ns;
    if (first_ns == 0ULL) {
        first_ns = mono_ns();
    }
    if (mono_ns() - first_ns < (uint64_t)HOOK_AFTER_MS * 1000000ULL) {
        return false;
    }
    *out_ns = BOARD_NS;
    return true;
#elif HOOK_ANSWERS == 3
    if (!hook_network_up) {
        return false;
    }
    *out_ns = BOARD_NS;
    return true;
#elif HOOK_ANSWERS == 4
    *out_ns = hook_calls == 1 ? BOARD_NS : BOARD_NS + BOARD_STEP_NS;
    return true;
#else
    (void)out_ns;
    return false;
#endif
}
