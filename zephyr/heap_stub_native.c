/*
 * Allocator glue for native_sim C/C++ API images (issue 1636).
 *
 * A Rust staticlib built with `std` for the host triple calls
 * `posix_memalign` for over-aligned allocations. Zephyr's
 * `lib/libc/common/malloc.c` (COMMON_LIBC_MALLOC) defines `malloc`/`calloc`/
 * `realloc`/`free`/`aligned_alloc`/`reallocarray` but not `posix_memalign` or
 * `memalign`, so the linker took them from picolibc's `libc.a` — and the
 * members that define them (`nano-malloc-posix_memalign`, `-memalign`,
 * `-malloc`) also define `malloc`, `free` and `aligned_alloc`: three
 * duplicates of Zephyr's allocator that `--allow-multiple-definition` used to
 * settle by link order. Measured with the flag removed: exactly those three
 * collided.
 *
 * Defining the two missing entry points HERE, on Zephyr's own allocator, means
 * no picolibc allocator member is pulled: one heap, and a duplicate is a link
 * error again.
 */
#include <errno.h>
#include <stddef.h>
#include <stdlib.h>

int posix_memalign(void **memptr, size_t alignment, size_t size)
{
    /* POSIX: a power of two and a multiple of sizeof(void *). */
    if (alignment < sizeof(void *) || (alignment & (alignment - 1)) != 0) {
        return EINVAL;
    }
    void *p = aligned_alloc(alignment, size);
    if (p == NULL && size != 0) {
        return ENOMEM;
    }
    *memptr = p;
    return 0;
}

void *memalign(size_t alignment, size_t size)
{
    return aligned_alloc(alignment, size);
}

/* picolibc's `sbrk` names these; no linker script on native_sim defines them.
 * Weak, so a board linker script that does define them wins. With the entry
 * points above, picolibc's allocator (the only `sbrk` caller) is not linked. */
__attribute__((weak)) char __heap_start;
__attribute__((weak)) char __heap_end;
