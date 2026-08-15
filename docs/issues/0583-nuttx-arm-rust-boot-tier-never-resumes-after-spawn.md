---
id: 583
title: "nuttx-arm Rust realtime entry: the boot tier never resumes after
  spawning, so the shared session is never flushed and the guest dies of lease
  expiry ~7 s in"
status: open
type: bug
area: boards
related: [issue-0579, issue-0572, issue-0569, issue-0246, rfc-0015, phase-281, phase-358]
---

## Symptom

Boot the prebuilt `workspace-rust-nuttx-realtime` fixture on QEMU arm-virt with
a `zenohd` on the baked locator. The guest console stops after the spawned tier
starts, and nothing from the BOOT tier ever appears again:

```
 1  nros entry ready
 2  nros: multi-tier run — 2 tier(s) over one session
 3  nros: boot tier `high` (session owner) — groups ["ctrl"], class Some("real_time"),
        budget Some(5000) us, period Some(10000) us, spin 1000 us, priority 110
 4  [INFO] Control::register on a tier admitting group `ctrl`
 5  [INFO] Telem::register on a tier admitting group `telem`
 6  nros: spawning tier `low` — groups ["telem"], class None, spin 10000 us
 7  nros: tier priority set tier=`low` prio=100
 8  nros: core pin FAILED tier=`low` cpu=0 — kernel lacks CONFIG_SMP, tier runs unpinned
 9  [INFO] Control::register on a tier admitting group `ctrl`
10  [INFO] Telem::register on a tier admitting group `telem`
11  nros: tier `low` entering spin — wake primitive available (16 byte(s))
12  [INFO] on_telem: first publish OK (tier `low` is dispatching)
13  nros: tier `low` completed spin 1
14  nros: tier `low` FIRST dispatch at spin 1 — 1 timer(s), 0 sub callback(s)
15  nros: RMW session open failed — ConnectionFailed
16  nros: Executor::open failed (Transport(ConnectionFailed)); multi-tier entry needs
        a live session — aborting.
```

Everything from line 7 on is the SPAWNED tier. The boot tier's own next
statement never runs.

## Why "the boot thread is stuck" and not "its output was lost"

`run_tiers` prints unconditionally right after the spawn loop when the boot tier
declares a budget:

```rust
if boot_is_budgeted {
    println!("nros: tier `{}` declares a sporadic budget but is the session-owning \
              boot tier — kept SCHED_FIFO …", boot_tier.name);
```

`boot_is_budgeted` is `class == Some("real_time") && budget_us.is_some() &&
period_us.is_some()`, and console line 3 shows all three present. Every
`println!` on this path is followed by an explicit `stdout().flush()` (issue
0572 made them stdout precisely so they reach the serial console). The line is
absent, so the boot thread did not get there — it stops at or inside
`Builder::spawn_scoped`, before its affinity, its priority and its spin.

## The consequence is the session, not just the tier

The boot tier is the SESSION OWNER: its spin drives the one shared zenoh-pico
session's TX flush for every tier (issue 0246). A packet dump of the guest NIC
shows what a stuck owner costs:

* exactly ONE TCP connection for the whole run — the zenoh handshake to
  `10.0.2.2:8291` completes and carries real traffic;
* the guest's last transmission is ~7 s in;
* the ROUTER then sends `FIN`, and retransmits `FIN,PSH` unanswered every 12 s.

So the spawned tier's `on_telem: first publish OK` never leaves the guest: it is
queued on a session nobody flushes, and the router drops the peer on lease
expiry. The guest is effectively dead from ~7 s.

Lines 15–16 are the aftermath, not the cause: `Executor::open` is called BEFORE
the `nros entry ready` banner, so a failing open cannot be the run that printed
lines 1–14. It is a later re-entry, and the dump shows it never emits a SYN (nor
a second ARP) — it fails before reaching the network.

## What it is NOT

* **Not the board.** The C++ arm of the SAME board and the SAME workspace
  (`realtime-cpp/…/nuttx_entry`, `[tiers.high]` 10 ms ctrl / `[tiers.low]`
  100 ms telem) runs the full 60 s with the expected ~10:1 ordering:
  `[ctrl] tick=31` against `[telem] tick=6`. The boot tier there spins fine.
* **Not issue 0579's fix.** Rebuilt the fixture with
  `packages/boards/nros-board-nuttx/src/lib.rs` reverted to `64fee4e60^` (the
  commit that added `apply_tier_priority(boot_tier)`) and the console is
  identical but for the `priority 110` field that commit also added. Pre-dates
  it.
* **Not the `pthread_attr_t` mirror overflow (0570).** `just
  check-nuttx-libc-struct-sizes` is green against the configured kernel —
  `pthread_attr_t` nuttx 56 B, mirror 56 B (`__PTHREAD_ATTR_SIZE__ = 14`) — and
  every other mirror covers its struct. Worth restating because 0570's fix is
  what made 0569 pass and the failure sits in the same `spawn` code.

## Why this was not caught

The row that would catch it, `realtime_tiers_e2e` `nuttx-arm/rust`, currently
SKIPS: its native peer fixture (`int32-sink`) reports STALE, and a skip is not a
pass. Issue 0569 records the row passing 16/16 on 2026-08-14, so either this
regressed after that date or that run's guest differed; the bisect is not done.
That is the first thing to establish, and it is why this is filed rather than
guessed at.

## Where to look

* `std::thread::Builder::spawn_scoped` on this target — the parent's post-return
  bookkeeping is the last place the boot thread is known to be. The child
  clearly ran, so `pthread_create` itself succeeded.
* The stdout path: every observed boot-tier statement after the spawn is a
  `println!`, so a lock held across the child's own logging would produce
  exactly this shape. `install_stdout_logger` + the `[INFO]` bridge changed in
  `27a8233d0` (phase-338 W7), which is inside the suspect window.
* Whether the boot thread is alive at all — a NuttX task table would settle
  stuck-vs-dead in one look. The SDK's `arm-none-eabi-gdb` cannot run on this
  host (`libncursesw.so.5` absent); `gdb-multiarch` is present and reads the
  same ELF, but QEMU's gdbstub shows CPUs, not NuttX tasks, so this needs the
  kernel's task list rather than a bare `bt`.

## Acceptance

* the boot tier's post-spawn statements execute — at minimum its
  `kept SCHED_FIFO` note, its priority adopt marker (issue 0579) and its spin;
* `/ctrl` is delivered off-guest, i.e. the shared session is flushed by its
  owner;
* `realtime_tiers_e2e` `nuttx-arm/rust` RUNS (not skips) and passes;
* the bisect either names the commit that regressed it or shows it never worked
  in the shape 0569 recorded.
