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

# --- freertos: vTaskCoreAffinitySet ----------------------------------------
head2 "freertos core-pin arm (vTaskCoreAffinitySet)"
if [ -z "${FREERTOS_DIR:-}" ] || [ ! -d "${FREERTOS_DIR}" ]; then
    nros_check_skip "check-sched-dim-arms(freertos)" "FREERTOS_DIR unset/absent (source ./activate.sh)"
elif ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
    nros_check_skip "check-sched-dim-arms(freertos)" "arm-none-eabi-gcc not on PATH"
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
elif ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
    nros_check_skip "check-sched-dim-arms(nuttx)" "arm-none-eabi-gcc not on PATH"
else
    out="$(arm-none-eabi-gcc -fsyntax-only -mcpu=cortex-a7 -mthumb-interwork \
        -DCONFIG_SMP \
        -I "$NUTTX_DIR/include" \
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

# --- threadx ----------------------------------------------------------------
# Not covered. The vendored ThreadX is the SINGLE-CORE port: it does not ship
# `tx_thread_smp_core_exclude` in any header, so there is no declaration to
# type-check our call against. Unlike freertos (a synthetic port config) and
# nuttx (one macro), this needs the SMP port sources vendored first — a
# different and larger job. Stated here so the gap is visible in the gate's own
# output, not only in the issue.
head2 "threadx core-pin arm (tx_thread_smp_core_exclude)"
say "NOT COVERED — issue 0260. The vendored ThreadX is the single-core port and"
say "ships no SMP header, so there is no declaration to check against."

echo
if [ "$fail" -ne 0 ]; then
    echo "check-sched-dim-arms-compile: FAILED"
    exit 1
fi
echo "check-sched-dim-arms-compile: OK"
