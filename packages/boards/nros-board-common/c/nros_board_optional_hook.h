/*
 * nros_board_optional_hook.h -- the ONE spelling of "call this optional board
 * hook if anything in the image defines it", for the board runner C files.
 *
 * An optional board hook is a `void name(void)` function whose default body is
 * a WEAK definition emitted by a public header (today: `nros_board_network_wait`,
 * in `<rclc/detail/main.h>`), and which a board may override strongly. A
 * header-emitted definition exists only in the TUs that include the header.
 * The board runners (`freertos_run_sched_tasks.c`, `zephyr_run_sched_tasks.c`,
 * `nuttx_run_sched_tasks.c`, `nros_rtos_run_components.c`) do not include it,
 * and an image whose entry does not either -- a hand-written C example, or an
 * unmodified ROS 2 C++ program built with `ROS2_MAIN` over `<rclcpp/rclcpp.hpp>`
 * -- has no TU that emits the default. If the runner is in the link and is not
 * garbage-collected (a host board with no `--gc-sections`, a whole-archived
 * seam library), a plain `extern` then fails the link:
 * `undefined reference to nros_board_network_wait`. ThreadX hit it first
 * (issue 1459), FreeRTOS POSIX next (issue 2761).
 *
 * One plain `extern` anywhere in the image is enough to fail it: GNU ld merges
 * the bindings of all references to a symbol, so a single non-weak reference
 * makes it a strong undefined and every reference site, weak ones included, is
 * reported. That is why every runner goes through this header and none keeps
 * its own declaration (`scripts/check-board-hook-weak-ref.py`).
 *
 * The fix is a WEAK REFERENCE (a declaration, not a definition) plus a guarded
 * call. It resolves to the header's weak default or to a board's strong
 * override whenever either is linked; when neither is, the default would have
 * been a no-op, so skipping the call is the same behaviour. The header keeps
 * the one body, so there is still exactly one definition of each hook.
 *
 * A weak-undefined function resolves to address 0. The guard reads that
 * address before calling, and GNU ld rewrites the address load for an
 * undefined weak symbol, so this links on RISC-V at 0x80000000 too (checked:
 * medany and medlow, riscv64-unknown-elf binutils with gcc 10.2) -- unlike an
 * UNGUARDED direct call, which `threadx_hooks.c` avoids for that reason.
 *
 * Usage, at file scope then at the call site:
 *
 *     NROS_BOARD_OPTIONAL_HOOK(nros_board_network_wait);
 *     ...
 *     NROS_BOARD_CALL_OPTIONAL_HOOK(nros_board_network_wait);
 *
 * Without GNU C there is no weak reference: the hook is a plain declaration
 * and is called unconditionally, which needs the header's default (or an
 * override) in the image, the behaviour before this header existed.
 */
#ifndef NROS_BOARD_OPTIONAL_HOOK_H
#define NROS_BOARD_OPTIONAL_HOOK_H

#if defined(__GNUC__) || defined(__clang__)
#define NROS_BOARD_OPTIONAL_HOOK(name) extern void name(void) __attribute__((weak))
#define NROS_BOARD_CALL_OPTIONAL_HOOK(name)                                                        \
    do {                                                                                           \
        if (name) {                                                                                \
            name();                                                                                \
        }                                                                                          \
    } while (0)
#else
#define NROS_BOARD_OPTIONAL_HOOK(name) extern void name(void)
#define NROS_BOARD_CALL_OPTIONAL_HOOK(name) name()
#endif

/* The optional hooks themselves, declared once here so a runner cannot spell
 * one as a plain `extern`. */

/* Network readiness: the weak no-op default is in `<rclc/detail/main.h>`; a
 * board whose link comes up after the entry starts overrides it strongly. */
NROS_BOARD_OPTIONAL_HOOK(nros_board_network_wait);

#endif /* NROS_BOARD_OPTIONAL_HOOK_H */
