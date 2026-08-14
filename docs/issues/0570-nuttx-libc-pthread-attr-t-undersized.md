---
id: 570
title: "Rust's NuttX `pthread_attr_t` is 20 bytes, NuttX's is 56 — every `pthread_attr_init`/`destroy` from Rust std smashes 36 bytes of the caller's frame"
status: open
type: bug
area: boards
related: [issue-0565, issue-0569, issue-0246, issue-0167, issue-0160, phase-285]
---

## Symptom

`realtime_tiers_e2e`, row `nuttx-riscv/rust`. The guest console (printed by the
verdict since issue 0565) is a NuttX assertion dump — pages of `stack_dump:`
followed by `dump_tasks:`. The task table diagnoses itself:

```
dump_task:  PID GROUP PRI POLICY  TYPE    … STACKBASE   STACKSIZE  USED    FILLED  COMMAND
dump_task:    3     3 100 RR      Task    … 0x800caea8      65208   58364   89.5%!  nsh_main
dump_task:    4     3 100 RR      pthread … 0x801225f0      65248     912    1.3%   nsh_main
dump_task:    5     3 100 RR      pthread … 0x801325f8      65240     356    0.5%   nsh_main
```

`nsh_main` at **89.5%** of 65 208 bytes, flagged `!`. The test reports
`low-tier /telem never reached 5 deliveries`, which is what a crashed guest looks
like from outside.

## Reading

The pressure is on the TASK that runs the entry, not on the spawned tiers — they
sit at 1.3% and 0.5% of comparable stacks. So this is not the `#246` tier-stack
problem (spawned tiers requesting std's 2 MiB default); those stacks are already
explicit and nearly empty.

What runs on the main task here is the boot tier: `Executor::open`, the RMW
session, the boot tier's declares, and then `nuttx_spin_tier_forever`. The Rust
executor plus zenoh-pico's call depth is the plausible consumer.

## Where to look

* the entry task's stack budget — the `nsh_main` stack size in the riscv
  defconfig (`packages/boards/nros-board-nuttx-qemu/nuttx-config/riscv/defconfig`),
  against what the arm cell gets (arm does NOT crash, so the two budgets or the
  two call depths differ — establish which);
* whether 89.5% is the peak or the point of death: `USED` is a high-water mark,
  and `!` means NuttX flagged it, so the overflow may be just past this frame;
* `CONFIG_PTHREAD_STACK_DEFAULT` is 64 KiB and the tiers honour `stack_bytes`
  (`NUTTX_TIER_STACK_DEFAULT_BYTES`) — the main task is the one path that does
  NOT go through that knob.

## Note

Sibling row `nuttx-arm/rust` fails in the SAME test with a completely different
mechanism (a transport failure — issue 0569). They were one issue (0565) until
the verdict started printing the console; do not assume a shared fix.

## Acceptance

* the `nuttx-riscv/rust` row of `realtime_tiers_e2e` passes;
* the fix states the measured headroom, not just "it stopped crashing" — a
  stack raised until the symptom disappears is issue 0163's shape.


## CAUSE FOUND 2026-08-13 — riscv gives the entry task 1/8 of what arm gives it

The two boards' defconfigs disagree by 8x on exactly the stack that overflowed:

```
packages/boards/nros-board-nuttx-qemu/nuttx-config/arm/defconfig
    CONFIG_INIT_STACKSIZE=524288          # 512 KiB
    CONFIG_PTHREAD_STACK_DEFAULT=65536

packages/boards/nros-board-nuttx-qemu/nuttx-config/riscv/defconfig
    CONFIG_INIT_STACKSIZE=65536           # 64 KiB
    CONFIG_SYSTEM_NSH_STACKSIZE=65536
    CONFIG_PTHREAD_STACK_DEFAULT=65536
```

The dump's `nsh_main` carried `STACKSIZE 65208` — 64 KiB less overhead, i.e.
riscv's budget. arm runs the SAME Rust entry (same executor, same zenoh-pico
call depth) on 512 KiB and does not crash. The pthread default is identical on
both, which is why the spawned tiers are fine on both (1.3% / 0.5%): the
asymmetry is only on the task that runs the entry.

So this is not a NuttX-riscv bug and not a Rust bug — it is one board's
defconfig never having been raised when the Rust entry's depth grew, while its
sibling's was.

### The arithmetic

| | riscv before | riscv after | arm |
| --- | --- | --- | --- |
| entry-task stack | 65 208 B | 524 288 B | 524 288 B |
| observed high-water | 58 364 B | 58 364 B | — |
| headroom | **10.5 %** | **88.9 %** | — |

58 364 of 65 208 is 89.5 % USED with NuttX's `!` flag. Against 512 KiB the same
high-water is 11.1 % used. The number is not chosen to make the symptom stop —
it is arm's existing budget, on the argument that the two boards run the same
entry and should not disagree about how much stack it needs.

### Change

`CONFIG_INIT_STACKSIZE` and `CONFIG_SYSTEM_NSH_STACKSIZE` both 65536 -> 524288,
matching arm.

### NOT VERIFIED — this is where the session ended

The change is applied but NOT proven: it needs a NuttX riscv kernel reconfigure
+ image rebuild and a `realtime_tiers_e2e` run, which did not fit. Until that
runs, this issue stays OPEN and the change is a hypothesis with arithmetic
behind it, not a fix.

What "verified" must mean here, per this issue's own acceptance: re-run and read
the NEW high-water out of a `dump_tasks:` (or a clean pass plus a deliberate
dump), and record it. A stack raised until the crash disappears, with no measured
headroom, is issue 0163's shape and is what that acceptance exists to prevent.


## REFUTED 2026-08-13 — the stack was not the cause

Provisioned (`nros setup qemu-riscv-nuttx`), rebuilt the riscv fixtures with the
raised budget, re-ran. The change reached the image and did exactly what the
arithmetic above predicted:

```
before   dump_task: 3 … 0x800caea8   65208  58364  89.5%!  nsh_main
after    dump_task: 3 … 0x800caec8  523960  58364  11.1%   nsh_main
```

Same high-water (58 364 B), 11.1 % of the new budget, and NuttX's `!` overflow
flag is gone.

**And the guest still crashes.** The row still fails with the same
`stack_dump:` + `dump_tasks:` assertion dump.

So the 89.5 % reading was a co-symptom, not the cause — the entry legitimately
uses ~57 KiB and was merely close to a budget that was too small, which is real
but is not what kills it. The hypothesis in the section above is REFUTED by its
own acceptance test, which is why that acceptance asked for measured headroom
rather than "the crash stopped".

### Keep the stack change anyway

It is not the fix and must not be described as one, but it stands on its own:
the riscv entry task now has the same 512 KiB its arm sibling has, running the
same Rust entry, with 88.9 % headroom instead of 10.5 %. Leaving one board at
1/8 of the other's budget for the same workload is a trap regardless of this
crash.

### What is actually needed next

The assertion LINE that precedes the dump — which this session did not capture,
because the verdict prints the LAST 25 lines and a NuttX dump is hundreds of
lines long, so the window lands in the middle of the `stack_dump:` hex and the
cause scrolls off the top. That is a defect in the diagnostic added by
`0d56d8bc9`: for a crash, the HEAD of the dump carries the reason
(`up_assert` / exception cause / the failing file:line) and the tail carries
only the task table.

Next step, in order:

1. make the guest drain report the FIRST lines of a dump as well as the last
   (or detect `stack_dump:`/`dump_tasks:` and keep the assertion header);
2. read the real assertion, then diagnose from it.

Do NOT chase the stack further: 11.1 % used with no overflow flag settles it.


## CAUSE, PROVEN 2026-08-14 — `pthread_attr_destroy` memsets 56 bytes into a 20-byte Rust object

Step 1 above (`console_excerpt`, head 30 + tail 20) printed the header the tail
had been hiding, on the first run:

```
nros: multi-tier run — 2 tier(s) over one session
riscv_exception: EXCEPTION: Instruction access fault. MCAUSE: 00000001, EPC: 00000000, MTVAL: 00000000
up_dump_register: EPC: 00000000
up_dump_register: S0: 00000000 S1: 00000000 S2: 00000000 S3: 00000000
up_dump_register: S4: 00000000 S5: 801563b8 S6: 80156d30 S7: 8014ac60
up_dump_register: SP: 80141060 FP: 00000000 TP: 00000000 RA: 00000000
```

Not a stack problem in any form: the guest **executed at address 0**. `RA` and
`S0`–`S4` are zero while `S5`–`S11` hold live values — a partial callee-saved
set, which names the exact epilogue that ran.

### The instruction that jumped to 0

`qemu-system-riscv32 -d exec,int` — the last translation block before
`desc=fault_fetch` is `0x80018f30`, the epilogue of
`std::sys::thread::unix::Thread::new`:

```
80018f22:  addi  a0,sp,8                     # &attr  (frame is 64 B: sp+0..63)
80018f24:  auipc ra,0x1e
80018f28:  jalr  708(ra)   # 800371e8 <pthread_attr_destroy>
80018f2c:  sw    a0,36(sp)
80018f2e:  bnez  a0,80018f7e
80018f30:  lw    ra,60(sp)                   # <-- restores the SIX registers
80018f32:  lw    s0,56(sp)                   #     that are zero in the dump
80018f34:  lw    s1,52(sp)
80018f36:  lw    s2,48(sp)
80018f38:  lw    s3,44(sp)
80018f3a:  lw    s4,40(sp)
80018f3c:  addi  sp,sp,64                    # -> 0x80141060, the dumped SP
80018f3e:  ret                               # -> 0
```

and the callee:

```
800371e8 <pthread_attr_destroy>:
800371e8:  beqz  a0,80037200
800371ec:  li    a2,56                       # <-- n = sizeof(pthread_attr_t)
800371f0:  li    a1,0
800371f4:  jal   8002f0ee <memset>
```

`attr` sits at `sp+8`. Rust reserved 20 bytes for it. NuttX writes **56**, i.e.
`sp+8 .. sp+63` — which is precisely `s4`(40), `s3`(44), `s2`(48), `s1`(52),
`s0`(56), `ra`(60). The epilogue then loads `ra = 0` and returns to it.

`pthread_attr_init` (`800371c6`) has the same shape — a `memcpy` of the same 56
bytes — so the frame is already smashed on the way IN; `destroy` only makes the
damage fatal by zeroing `ra`.

### Why the two sizes disagree

`pthread_attr_t` is Kconfig-dependent in NuttX (`include/pthread.h`):
`CONFIG_SCHED_SPORADIC` appends `repl_period` + `budget` (two `struct timespec`,
16 B each under `CONFIG_SYSTEM_TIME64`), `CONFIG_SMP` appends `affinity`. The
Rust mirror hardcodes ONE layout:

```rust
// third-party/nuttx/libc/src/unix/nuttx/mod.rs
const __PTHREAD_ATTR_SIZE__: usize = 5;      // 5 * 4 = 20 B on ilp32
pub struct pthread_attr_t { __val: [usize; __PTHREAD_ATTR_SIZE__] }
```

20 bytes is exactly the `SCHED_SPORADIC=n`, `SMP=n` layout. **Both** NuttX QEMU
boards set `CONFIG_SCHED_SPORADIC=y` (the tier model needs the sporadic server,
#246) plus `CONFIG_SYSTEM_TIME64=y`, so the real struct is 56 bytes. Measured
against the built headers rather than counted by hand:

```
riscv-none-elf-gcc -isystem third-party/nuttx/nuttx/include -c sizeprobe.c
  probe_pthread_attr_t   0x38   = 56
```

### This is #167's class, in the same file

The fork already carries a `--wrap=poll` shim because NuttX's kernel `pollfd` is
24 bytes and Rust's is 8, and `poll()` "overflows a caller's array by 16 bytes
per entry and smashes whatever follows" — its own words, at
`src/unix/nuttx/mod.rs:601`. Same defect, same fork, different struct, found the
hard way twice. So the fix is not one constant; it is the constant **plus a gate
that measures every mirrored type against the headers.**

### Sweep — every mirrored NuttX type, measured not eyeballed

`sizeof` from the built NuttX headers (riscv32, this config) vs what the fork
reserves. Undersized is a stack/heap smash; oversized is harmless.

| type | NuttX | fork | |
| --- | --- | --- | --- |
| `pthread_attr_t` | 56 | `[usize; 5]` = 20 | **UNDERSIZED by 36** |
| `pthread_mutex_t` | 28 | `[usize; 9]` = 36 | ok |
| `pthread_cond_t` | 24 | `[usize; 7]` = 28 | ok |
| `pthread_condattr_t` | 8 | `[usize; 5]` = 20 | ok |
| `pthread_rwlock_t` | 64 | `[usize; 17]` = 68 | ok |
| `sem_t` | 16 | `[usize; 6]` = 24 | ok |
| `fd_set` | 32 | `[u32; 10]` = 40 | ok |
| `sigset_t` | 8 | `[u32; 8]` = 32 | ok |
| `sockaddr_storage` | 128 | `2 + [u32; 36]` = 148 | ok |
| `dirent.d_name` | 33 | `[c_char; 65]` | ok |
| `struct pollfd` | 24 | 8 | undersized — already handled by `--wrap=poll` (#167) |

`pthread_attr_t` is the only unhandled one.

### arm has the identical defect — check #569 against it before diagnosing it separately

`arm-none-eabi-objdump` on the arm fixture shows the same `mov r2, #56` in
`pthread_attr_destroy`, and arm's `Thread::new` puts `attr` at `sp+0` of a
32-byte local area under `push {r4, r5, r6, r7, r9, lr}` — so the 56-byte write
runs 24 bytes past, over all six pushed registers **including `lr`**. The write
is real on arm; only where it lands differs. #569 (`Transport(ConnectionFailed)`)
must be re-tested against the fix before being treated as an independent bug.
NOT established: that #569 IS this — only that this defect is live on arm too.

### Fix

1. `__PTHREAD_ATTR_SIZE__` in the fork must cover the largest NuttX layout, not
   the smallest. 14 usizes = 56 B on ilp32; oversizing is safe (C touches only
   its own `sizeof`), undersizing is a smash.
2. A gate that COMPILES a size probe against the configured NuttX headers and
   compares every mirrored type against the fork's constants — the sweep above,
   automated, so the third instance of this class fails the build instead of
   costing a kernel-dump bisect. #167 fixed one struct and left the rule
   unenforced; that is why this one survived.

## FIXED + VERIFIED 2026-08-14

`__PTHREAD_ATTR_SIZE__` 5 -> 14 in the fork. The frames move exactly as the
diagnosis predicts — riscv `Thread::new` goes from a 64-byte frame with `attr`
at `sp+8` and `s4` at `sp+40` (a 20-byte overlap) to 96 bytes with `attr` at
`sp+4` and `s4` at `sp+72`; arm's local area goes 32 -> 72 bytes under the same
`push {r4, r5, r6, r7, r9, lr}`. Both now clear the 56-byte write.

`realtime_tiers_e2e`: **16 row(s) ran, 0 skipped, 0 out of lane**, all pass
(147 s, re-measured after #571 stopped the lane silently skipping cells) —
including `nuttx-arm/rust`. That row was BOTH #569 (the session open failing)
and #572 (the high tier delivering zero): the same smash landing on different
registers, exactly as the section above suspected. Both are closed by this.

On arm the corrupted tier is the BOOT one, because it is the CALLER of
`Thread::new` — which is why #572 saw the FAST tier silent and the spawned slow
tier healthy, the opposite of what the symptom suggests.

The gate is `scripts/check-nuttx-libc-struct-sizes.py` (`just
check-nuttx-libc-struct-sizes`, and `just nuttx ci`, where a configured kernel
and a cross compiler are guaranteed — in `check-fast` it degrades to a loud
NOT CHECKED rather than a silent pass). It compiles a `sizeof` probe against the
NuttX headers and fails any mirror smaller than its struct; self-tested by
reverting the constant, which reproduces the exact failure and prescribes
`>= 14`.

### One thing NOT fixed here

The superproject pins the libc submodule at `adb4c59` (#167's `--wrap=poll`
shim), and **no branch on the fork remote contains it** — `git fetch` leaves
`origin/main` at upstream's `2aa834e`. So the pin is already unresolvable from a
fresh clone, and this fix sits on top of it. Both commits need pushing to the
fork before the superproject pointer can move (the agent does not push fork
remotes). Filed separately.

## Status

Cause proven, fix applied and verified. Reproduces standalone and
deterministically **before** the fix:

```
build/zenohd/zenohd -l tcp/0.0.0.0:17867 -l tcp/0.0.0.0:8691 --no-multicast-scouting &
qemu-system-riscv32 -M virt -bios none -nographic -icount shift=auto \
  -kernel examples/workspaces/realtime-rust/target-fixtures/nuttx-riscv/\
riscv32imac-unknown-nuttx-elf/nros-minsizerel/riscv_nuttx_entry \
  -netdev user,id=net0 -device virtio-net-device,netdev=net0
```

The 512 KiB stack change stands (arm parity, 88.9 % headroom) but is NOT the
fix and is verified inert w.r.t. this crash.
