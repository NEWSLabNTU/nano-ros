#!/usr/bin/env bash
# Issue 0260 / phase-356 W3 — compile the sched-dim ACCEPT arms that no build
# otherwise reaches.
#
# `sched_dims_applied_e2e` declares an expected arm per cell, and several cells
# expect the FALLBACK because the accept-arm code sits behind an SMP macro no
# image defines:
#
#   freertos  #if configUSE_CORE_AFFINITY == 1   -> vTaskCoreAffinitySet
#   nuttx     #ifdef CONFIG_SMP                  -> pthread_setaffinity_np
#   threadx   #ifdef TX_THREAD_SMP               -> tx_thread_smp_core_exclude
#
# The fallback is honestly reported at runtime, so the TEST is not lying. What
# is missing is any build at all: an API misuse in those blocks — wrong arity,
# wrong argument order, a renamed upstream symbol — is invisible, because the
# compiler never sees the code. That is the class #0260 names.
#
# This is a SYNTAX-ONLY compile (`-fsyntax-only`): it type-checks our call sites
# against the vendored RTOS headers' REAL declarations. It does not link and it
# does not run. Making one of these arms actually EXECUTE and be observed
# accepting needs a genuine SMP board, which is phase-356's separate, larger
# item — do not confuse a green here with that.
#
# ## Why a synthetic SMP config, and what it costs
#
# The phase doc says this half needs no SMP. Measured, that is not true for
# FreeRTOS: `configUSE_CORE_AFFINITY=1` alone is rejected by the kernel
# (`#error configUSE_CORE_AFFINITY is not supported in single core FreeRTOS`),
# and `configNUMBER_OF_CORES=2` then demands port primitives the ARM_CM3 port
# does not define. So the check supplies a minimal synthetic SMP port.
#
# The honest cost: those stubs describe a port that does not exist. If upstream
# adds a newly-required SMP macro, this check breaks for a reason that has
# nothing to do with our code — so that case is detected and reported as
# STUB-DRIFT rather than as a failure of the call site.
set -o pipefail
cd "$(dirname "$0")/.."
repo_root="$(pwd)"

# issue 0650 — a gate that could not run must SAY so through the shared ledger,
# or `check`'s closing "All checks passed!" stands for something that never
# executed. This gate skips on an unprovisioned host by design (no RTOS sources,
# no cross compiler), which is exactly the case the ledger exists for.
# shellcheck source=scripts/build/check-skip.sh
. "$repo_root/scripts/build/check-skip.sh"

fail=0

say()  { printf '  %s\n' "$*"; }
head2() { printf '\n== %s ==\n' "$*"; }

# A cross gcc on PATH is not a cross gcc that can COMPILE: the CI container
# ships the bare binary without newlib, so `<string.h>` is absent and every
# arm FAILs on a header this gate is not testing (the cross-libc gate already
# skips on the same container for the same reason). Probe usability once —
# presence of the compiler AND its libc headers — and let each arm skip
# legibly on the same wording.
arm_gcc_usable=""
arm_gcc_unusable_why=""
if command -v arm-none-eabi-gcc >/dev/null 2>&1; then
    if printf '#include <string.h>\n#include <stdlib.h>\nint main(void){return 0;}\n' \
        | arm-none-eabi-gcc -fsyntax-only -x c - >/dev/null 2>&1; then
        arm_gcc_usable=1
    else
        arm_gcc_unusable_why="arm-none-eabi-gcc has no usable newlib (<string.h>/<stdlib.h> absent)"
    fi
else
    arm_gcc_unusable_why="arm-none-eabi-gcc not on PATH"
fi

# --- freertos: vTaskCoreAffinitySet ----------------------------------------
head2 "freertos core-pin arm (vTaskCoreAffinitySet)"
if [ -z "${FREERTOS_DIR:-}" ] || [ ! -d "${FREERTOS_DIR}" ]; then
    nros_check_skip "check-sched-dim-arms(freertos)" "FREERTOS_DIR unset/absent (source ./activate.sh)"
elif [ -z "$arm_gcc_usable" ]; then
    nros_check_skip "check-sched-dim-arms(freertos)" "$arm_gcc_unusable_why"
else
    out="$(arm-none-eabi-gcc -fsyntax-only -mcpu=cortex-m3 -mthumb \
        -DconfigUSE_CORE_AFFINITY=1 \
        -DconfigNUMBER_OF_CORES=2 \
        -DconfigUSE_PASSIVE_IDLE_HOOK=0 \
        '-DportGET_CORE_ID()=0' \
        '-DportYIELD_CORE(x)=(void)(x)' \
        '-DportSET_INTERRUPT_MASK()=0' \
        '-DportCLEAR_INTERRUPT_MASK(x)=(void)(x)' \
        '-DportGET_TASK_LOCK(x)=(void)(x)' \
        '-DportRELEASE_TASK_LOCK(x)=(void)(x)' \
        '-DportGET_ISR_LOCK(x)=(void)(x)' \
        '-DportRELEASE_ISR_LOCK(x)=(void)(x)' \
        '-DportENTER_CRITICAL_FROM_ISR()=0' \
        '-DportEXIT_CRITICAL_FROM_ISR(x)=(void)(x)' \
        -I "$repo_root/packages/boards/nros-board-mps2-an385-freertos/config" \
        -I "$FREERTOS_DIR/include" \
        -I "$FREERTOS_DIR/portable/${FREERTOS_PORT:-GCC/ARM_CM3}" \
        -I "$repo_root/packages/api/nros-c/include" \
        -I "$repo_root/packages/platform/nros-platform-api/include" \
        "$repo_root/packages/boards/nros-board-freertos/c/freertos_run_tiers.c" 2>&1)"
    rc=$?
    if [ "$rc" -eq 0 ]; then
        say "OK: the accept arm type-checks against FreeRTOS $(grep -o 'V[0-9.]*' "$FREERTOS_DIR/include/task.h" | head -1)"
    elif printf '%s' "$out" | grep -qE 'is required in SMP|must also be defined|Missing definition'; then
        # Not our code: the vendored kernel wants a port macro this synthetic
        # config does not supply. Say so, loudly, rather than blaming the arm.
        fail=1
        say "STUB-DRIFT: the vendored FreeRTOS now requires an SMP port macro this"
        say "            check does not stub. Add it to the -D list above; the call"
        say "            site is NOT implicated. Compiler said:"
        printf '%s\n' "$out" | grep -E 'error' | head -3 | sed 's/^/            /'
    else
        fail=1
        say "FAIL: the accept arm does not compile — this IS the call site."
        printf '%s\n' "$out" | grep -E 'error' | head -5 | sed 's/^/      /'
    fi
fi

# --- nuttx: pthread_setaffinity_np -----------------------------------------
#
# Cheaper than FreeRTOS: the generated `nuttx/config.h` simply has no
# CONFIG_SMP (0 occurrences — it is a single-core defconfig), and NuttX gates
# the declaration on `#ifdef CONFIG_SMP` alone. Defining it on the command line
# exposes `cpu_set_t` / `CPU_ZERO` / `CPU_SET` / `pthread_setaffinity_np`
# together, with no synthetic port needed. No STUB-DRIFT arm here for that
# reason: there are no stubs to drift.
head2 "nuttx core-pin arm (pthread_setaffinity_np)"
if [ -z "${NUTTX_DIR:-}" ] || [ ! -d "${NUTTX_DIR}/include" ]; then
    nros_check_skip "check-sched-dim-arms(nuttx)" "NUTTX_DIR unset/absent (source ./activate.sh)"
elif [ -z "$arm_gcc_usable" ]; then
    nros_check_skip "check-sched-dim-arms(nuttx)" "$arm_gcc_unusable_why"
elif [ ! -f "$NUTTX_DIR/nros-nuttx-export-arm/include/nuttx/config.h" ]; then
    # Issue 0525. The tree is SHARED SOURCE, but `nuttx/config.h` is build
    # OPTIONS, and one shared copy cannot hold two arches: it belongs to
    # whichever was configured LAST. This arm compiles -mcpu=cortex-a7, so
    # taking the shared copy means type-checking ARM against whatever riscv
    # last wrote — silently, and it passes, which is worse than not running.
    # Hence a SKIP rather than a fallback: no arm snapshot, no verdict.
    nros_check_skip "check-sched-dim-arms(nuttx)" \
        "no arm NuttX export snapshot (nros-nuttx-export-arm); the shared tree's config.h is another arch's"
else
    # Per-arch build options come from this arch's export snapshot; the shared
    # tree supplies only source. Same split as
    # `nros_build_paths::nuttx_include_root` and cmake's
    # `nros_nuttx_include_root`, neither of which shell can call.
    nuttx_inc="$NUTTX_DIR/nros-nuttx-export-arm/include"
    out="$(arm-none-eabi-gcc -fsyntax-only -mcpu=cortex-a7 -mthumb-interwork \
        -DCONFIG_SMP \
        -I "$nuttx_inc" \
        -I "$NUTTX_DIR/sched" \
        -I "$repo_root/packages/api/nros-c/include" \
        -I "$repo_root/packages/platform/nros-platform-api/include" \
        "$repo_root/packages/boards/nros-board-nuttx-qemu/c/nuttx_run_tiers.c" 2>&1)"
    rc=$?
    if [ "$rc" -eq 0 ]; then
        say "OK: the accept arm type-checks against the vendored NuttX headers"
    else
        fail=1
        say "FAIL: the accept arm does not compile — this IS the call site."
        printf '%s\n' "$out" | grep -E 'error' | head -5 | sed 's/^/      /'
    fi
fi

# --- threadx: tx_thread_smp_core_exclude ------------------------------------
#
# This gate previously said threadx was NOT COVERABLE because "the vendored
# ThreadX is the single-core port and ships no SMP header". That was wrong: the
# vendored tree carries `common_smp/` AND `ports_smp/` (a5/a34/a35/a53, linux,
# mips...), so the real declaration is right there. Nothing needed vendoring.
#
# Port choice is load-bearing, and the wrong one fails for an unrelated reason.
# `ports_smp/linux/gnu` looks obvious (host gcc, no cross toolchain) and trips
# this file's own static assertion — its `ULONG` is `unsigned long`, 8 bytes on
# x86_64, and `threadx_hooks.c` asserts 4 (phase-337 W4.a). An ARM32 SMP port
# has a 4-byte ULONG and satisfies it, so `cortex_a5_smp` is used with the same
# arm-none-eabi-gcc the other arms need.
head2 "threadx core-pin arm (tx_thread_smp_core_exclude)"
if [ -z "${THREADX_DIR:-}" ] || [ ! -d "${THREADX_DIR}/common_smp/inc" ]; then
    nros_check_skip "check-sched-dim-arms(threadx)" "THREADX_DIR unset, or no common_smp/inc (source ./activate.sh)"
elif [ -z "$arm_gcc_usable" ]; then
    nros_check_skip "check-sched-dim-arms(threadx)" "$arm_gcc_unusable_why"
else
    out="$(arm-none-eabi-gcc -fsyntax-only -mcpu=cortex-a7 \
        -DTX_THREAD_SMP \
        -I "$THREADX_DIR/common_smp/inc" \
        -I "$THREADX_DIR/ports_smp/cortex_a5_smp/gnu/inc" \
        -I "$repo_root/packages/api/nros-c/include" \
        -I "$repo_root/packages/platform/nros-platform-api/include" \
        "$repo_root/packages/boards/nros-board-common/c/threadx_hooks.c" 2>&1)"
    rc=$?
    if [ "$rc" -eq 0 ]; then
        say "OK: the accept arm type-checks against the vendored ThreadX SMP headers"
    elif ! printf '%s' "$out" | grep -E 'error|note' | grep -q 'threadx_hooks[.]c'; then
        # NOTE lines count, and that is the whole subtlety. These APIs are
        # MACROS, so a bad call is blamed on the macro DEFINITION in the vendor
        # header, and our file is named only by the follow-up
        # `note: in expansion of macro ...`. Inspecting error lines alone
        # therefore reports a call-site fault as a port fault — measured, after
        # writing it that way first.
        #
        # No diagnostic names OUR translation unit, so the failure is in the
        # PORT/vendor headers, not the call site. Classifying by WHICH FILE the
        # compiler blamed beats matching one symptom: the two wrong-port
        # failures look nothing alike — host gcc + linux port trips this file's
        # 4-byte-ULONG assertion (phase-337 W4.a), while arm-none-eabi + linux
        # port dies on `semaphore.h: No such file`, a bare-metal toolchain
        # having no POSIX headers. Keying on either string alone misses the
        # other and misreports it as a call-site fault.
        fail=1
        say "PORT-MISMATCH: the chosen ports_smp port is not usable with this"
        say "               compiler — every diagnostic is in vendor headers,"
        say "               none in threadx_hooks.c. Call site NOT implicated."
        printf '%s\n' "$out" | grep 'error' | head -2 | sed 's/^/               /'
    else
        fail=1
        say "FAIL: the accept arm does not compile — this IS the call site."
        printf '%s\n' "$out" | grep -E 'error' | head -5 | sed 's/^/      /'
    fi
fi

echo
if [ "$fail" -ne 0 ]; then
    echo "check-sched-dim-arms-compile: FAILED"
    exit 1
fi
echo "check-sched-dim-arms-compile: OK"
