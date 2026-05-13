# Phase 121.3.freertos-parity — Regression Audit

**Status:** Deferred. Regression reproduces; root cause partially diagnosed via QEMU + gdb-multiarch.

**Goal:** Restore FreeRTOS Rust E2E (`test_rtos_pubsub_e2e::platform_1_Platform__Freertos::lang_1_Lang__Rust` + action / service variants) after 121.3.deprecate-rust-remove deleted the Rust kernel crate. Talker / listener hard-fault before publishing.

## Net regression

| Test | Pre-deletion | Post-deletion |
|---|---|---|
| `test_rtos_pubsub_e2e::Freertos::Rust` | pass | hard-fault (0 msgs received) |
| `test_rtos_action_e2e::Freertos::Rust` | pass | hard-fault |
| `test_rtos_service_e2e::Freertos::Rust` | pass | hard-fault |

552/572 → 549/572 in `just test`. Net delta: **−3**.

## Diagnostic sequence

1. **Network init never completes.** Standalone QEMU shows `Initializing LAN9118 + lwIP... / MAC: ... / IP: ...` then hangs. `Network ready` never prints.

2. **Hard-fault confirmed via gdb-multiarch.** Attached to running QEMU, target halts in `Default_Handler` at `0x0000ac34`. Fault-status registers:
   - `HFSR = 0x40000000` (FORCED — escalated from configurable fault).
   - `CFSR = 0x00020000` (UFSR bit 1 = INVSTATE — Invalid Execution State).
   - `LR = 0xfffffffd` (EXC_RETURN: thread mode + PSP).

3. **MSP frame stacked on fault entry.** SP = `0x203fffe0`, 8 hardware-stacked words:
   ```
   R0  = 0x20400000   (initial MSP — set by prvPortStartFirstTask)
   R1  = 0x00000007
   R2  = 0x00000000
   R3  = 0xe000e000   (SCB/NVIC region pointer)
   R12 = 0x000350a7
   LR  = 0x00017e2d   (xPortStartScheduler + 208)
   PC  = 0x00017c00   (prvPortStartFirstTask + 24, i.e. nop after svc 0)
   xPSR = 0x21000000  (T-bit set, exception num 0 = thread mode)
   ```

4. **First task to start IS picked correctly.** Break at `*0x17bdc` (prvPortStartFirstTask entry) → `pxCurrentTCB` resolves to a task TCB, `pxTopOfStack` points at a properly-prepared initial frame.

5. **SVC handler EXC_RETURN flip works.** Break at `*0x17bde` (orr) shows `lr = 0xfffffff9` (thread+msp). Break at `*0x17be2` (bx lr) shows `lr = 0xfffffffd` (thread+psp). `msr PSP, r0` sets PSP to the new task's hw-frame.

6. **First task runs.** Break at `prvTimerTask` (the first selected task — priority 2, same as app, but ready-list head) fires. PC = `prvTimerTask + 30`, SP on PSP. xPSR `0x01000000` (T-bit set). Timer task executes.

7. **nros_app TCB stack-frame initially correct.** `pxPortInitialiseStack(r0=0x20024A48, r1=0x8d61=app_task_entry, r2=pvParameters)` returns `pxTopOfStack=0x20024A08`. Frame slots verified at `vTaskStartScheduler` entry:
   ```
   0x20024A40 = 0x00008D60      ← PC slot (app_task_entry, Thumb-cleared) ✓
   0x20024A44 = 0x01000000      ← xPSR slot ✓
   0x20024A3C = 0x00017C4D      ← LR slot (prvTaskExitError+1) ✓
   ```

8. **Watchpoint on `0x20024A44` fires at `app_task_entry + 2`.** nros_app *does* execute its first instruction. The prologue's `push {…, lr}` decrements SP from `0x20024A48` to `0x20024A44` and overwrites the (now-reclaimed) xPSR slot with the stacked LR (`0x00017C4D`). This is **normal task-prologue stack usage**, not corruption.

## What is NOT the cause

Each of the following was inspected directly + ruled out:

- **Initial frame setup.** `pxPortInitialiseStack` writes PC / xPSR / LR / R0 correctly. Verified at break before and after the function.
- **SVC handler.** Loads task context cleanly, EXC_RETURN flipped to `0xfffffffd`.
- **Task priorities.** Both timer and nros_app at priority 2; timer wins because created later. Both start.
- **Stack overflow at boot.** Bumped `configCHECK_FOR_STACK_OVERFLOW = 2` + custom hook — hook never fires. (Side effect: 2× context-switch overhead slowed QEMU enough to mask later faults; reverted to 0.)
- **`xSemaphoreCreateRecursiveMutex` semantics.** `nros_platform_mutex_lock` calls `xQueueTakeMutexRecursive` (queue-type-4). Disasm-verified.
- **Critical-section ABI.** Phase 121.9 shim lands `nros_platform_critical_section_{acquire,release}` correctly across all C ports.

## Remaining hypothesis

Hardware unstack during a **later** PendSV context-switch back to nros_app reads a corrupted hw-frame. Symptoms:

- Fault occurs after timer + nros_app have run.
- Fault entry stacks to **MSP** (`0x203fffe0`), not PSP. Implies fault happened while CPU was already on MSP — i.e. during hardware unstack itself (the unstack runs in handler-finalisation, still on MSP). INVSTATE during hw-unstack matches this profile: popped xPSR with T-bit clear → fault entry pushes a new frame to current SP (still MSP).
- The stacked PC `0x00017c00` is `prvPortStartFirstTask + 24` — the address that was on MSP from the *original SVC stacking*. Hardware did **not** push a fresh frame; the existing MSP frame is what we read. The fault came in DURING unstack so MSP wasn't actually re-stacked (some implementations or stack mismanage edge cases).

Most likely the saved-from-nros_app PSP that PendSV puts back into `pxCurrentTCB->pxTopOfStack` is misaligned by 4 bytes, so on the next restore, hardware reads xPSR from the LR slot and PC from the R12 slot — both have arbitrary 32-bit values without bit-24 = 1 → INVSTATE on hw-unstack.

## Possible causes worth probing next session

1. **FPU register-save in `xPortPendSVHandler`.** Cortex-M3 has no FPU, so `vstmdbeq` etc. should be no-ops or absent. If FreeRTOS V11.2.0's port.c compiled with M4F FPU sentinel by mistake, it would shift PSP by 16 bytes (FPU regs) on save and a different amount on restore.

2. **`configTOTAL_HEAP_SIZE = 2MB` + heap_4 fragmentation.** All TCBs + stacks share one `ucHeap`. If something else heap-mallocs into nros_app's stack range, frame gets corrupted.

3. **Compiler / build flag drift** between when the test passed (Rust kernel crate active) and now. The C port is built by `nros-board-mps2-an385-freertos/build.rs` via cc-rs; the FreeRTOS kernel itself is built by the same build.rs from `third-party/freertos/kernel/`. Could compare object hashes against a checkout from `825b1cdd`'s build dir.

4. **`zpico-sys` build flags.** `phase-115.M.3` (merged cffi shims into Rust-impl crates) and `phase-122.x` (event-driven wake) landed via rebase. They may flip a feature that changes the zenoh-pico C transport's task model or stack expectations.

## Tooling

- `gdb-multiarch` (Ubuntu 12.1 package): handles ARM architecture cleanly.
- QEMU GDB stub: `qemu-system-arm ... -gdb tcp::1234` (omit `-S` to start free-running; attach later for halt-on-fault state).
- `tshark -i lo -f "port 7451"`: confirms zero TCP traffic from talker (slirp NAT goes to host lo).
- Inline asm semihosting `bkpt #0xAB` with `r0 = 4` (SYS_WRITE0) for `vApplicationStackOverflowHook` / `vApplicationMallocFailedHook` print path.

## Re-entry checklist

- Branch: `main` at `7065d2ef`. Frame-init parity fixes already on-tree (commits `825063d4`, `aa1dcafb`, `825b1cdd`, `edc0e97a`, `2f5fb8d6`, `6a64a478`).
- Repro: `cd examples/qemu-arm-freertos/rust/zenoh/talker && cargo build --release` then `qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic -icount shift=auto -semihosting-config enable=on,target=native -kernel target/thumbv7m-none-eabi/release/qemu-freertos-talker -nic user,model=lan9118`. Hangs after `IP: 10.0.2.20`.
- E2E: `cargo nextest run -p nros-tests -E 'binary(rtos_e2e) and test(test_rtos_pubsub_e2e::platform_1_Platform__Freertos::lang_1_Lang__Rust)'`. Fails in ~50s with `0 messages received`.

## Disposition

Deferred. Tracked under this audit doc + the open-item entry in `phase-121-platform-c-abi-canonical.md` (121.3.freertos-parity.remaining). Sister regression `121.3.threadx-linux-net` tracked separately.
