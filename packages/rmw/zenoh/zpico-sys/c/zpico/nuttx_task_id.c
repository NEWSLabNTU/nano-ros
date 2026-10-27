/**
 * NuttX task identity for zenoh-pico 1.9's background executor (RFC-0110,
 * phase-508).
 *
 * The executor asks "is the calling thread the one running me?" before it
 * takes its own lock, through `_z_task_get_id` / `_z_task_current_id` /
 * `_z_task_id_equal`. On NuttX the vendor header is `unix.h`, where
 * `_z_task_t` and `_z_task_id_t` are both `pthread_t`, and the NuttX task
 * storage holds exactly a `pthread_t`. POSIX gets the same three from
 * `platform_aliases.c`; NuttX's live here because this TU is compiled against
 * NuttX's headers and the alias TU is not (no `pthread_self` in scope there).
 *
 * A TU of its own because task identity and the clock are separate concerns.
 * (It was first split out because `nuttx_clock.c` was never linked and adding
 * any symbol there duplicated the alias TU's clock. That was the defect, not a
 * constraint: the alias clock had the wrong ABI for NuttX, issue 2094.)
 */

#include <pthread.h>
#include <stdbool.h>
#include <zenoh-pico/system/platform/unix.h>

_z_task_id_t _z_task_get_id(const _z_task_t *task) { return *task; }

_z_task_id_t _z_task_current_id(void) { return pthread_self(); }

bool _z_task_id_equal(const _z_task_id_t *l, const _z_task_id_t *r) { return pthread_equal(*l, *r) != 0; }
