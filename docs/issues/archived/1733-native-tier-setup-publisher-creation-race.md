---
id: 1733
title: "A tiered NATIVE image intermittently loses whole tiers at boot: concurrent tier setups over one shared session fail publisher creation (C++ `create_publisher_in` rc -3, Rust `PublisherCreationFailed`)"
status: resolved
type: bug
severity: medium
area: [runtime, native, tiers, zenoh]
related: [phase-463, 1597, 1571, 1711, 1749, 0447]
found: 2026-10-07
---

## Measured

phase-463 W7's profiler joins a profiled run against the census, and that join
refused two in-tree images for a reason that has nothing to do with profiling.
It reproduces on an `origin/main` build with the profiler absent.

**C++, `examples/workspaces/derived-tiers-cpp`, image `native`.** There are four
derived tiers over one zenoh session, against a live `rmw_zenohd`. Each run
lost 0 to 2 tiers at setup, and WHICH tiers varied from run to run:

```
[ERROR] .../nros-cpp/include/nros/node.hpp:315 node "mrm_handler": FAILED at create_publisher_in (code=-3)
[nros] FATAL: node "mrm_handler" failed to construct at create_publisher_in (code=-3)
[ERROR] nros: tier 'derived-mrm_handler' setup FAILED (rc=-3) — tier will not run
```

| build | run | tiers lost |
| --- | --- | --- |
| `origin/main` (no profiler) | 1 | `mrm_handler` |
| `origin/main` (no profiler) | 2 | `mrm_comfortable_stop_operator`, `mrm_handler` |
| W7 branch, profiling off | 1 | `mrm_handler` |
| W7 branch, profiling on | 1 | `mrm_comfortable_stop_operator`, `mrm_handler` |
| W7 branch, profiling on (60 s) | 1 | none |

The rate, from 6 s runs under one router (`NROS_ENTRY_SPIN_MS=6000`). "Lost"
means a tier setup FAILED and the run continued; "crash" means the process
died on SIGSEGV or SIGABRT:

| build | runs | ok | lost tier(s) | crash |
| --- | --- | --- | --- | --- |
| `origin/main` | 24 | 9 | 11 | 4 |
| W7 branch, profiling off | 8 | 3 | 2 | 3 |
| W7 branch, profiling on | 8 | 6 | 1 | 1 |

About 60 % of boots of this image are bad on `origin/main` itself.

**Rust, `examples/workspaces/realtime-rust`, image `native_derived`.** There
are two tiers. The control tier was lost in 3 of 3 runs on the W7 branch. No
`origin/main` build of this image was run, and unlike the C++ image the loss
did not vary. `Control::register` is logged twice (the boot tier, then the
control tier), so this half may be a deterministic duplicate registration
rather than the race:

```
[ERROR] nros: node declaration failed — NodeError::Transport(PublisherCreationFailed)
nros: tier `derived-control_node` setup failed: NodeRegister("ctrl_pkg") — tier task exiting
```

**It can also be heap corruption.** One run of the C++ image, on the W7 branch
with profiling on, died outright. Under gdb that run aborted with
`malloc(): unaligned tcache chunk detected`:

* tier thread 5 was inside zenoh-pico's `_z_slist_new_empty` (`z_malloc` ->
  `nros_platform_alloc`) during setup;
* thread 6 was in `_z_declare_liveliness_token` for `mrm_handler`, waiting on
  the session mutex;
* the run first logged `z_declare_publisher failed: -78` for the
  `/diagnostics` publisher, which is the SAME one every tier creates.

So concurrent tier setups corrupt the heap through the shared zenoh-pico
session, not merely refuse a create. The profiler changes the setup's timing
and touches none of that code, which is why the symptom moves between a lost
tier and a crash.

## What it is not

* **It is not `nros_cpp_publisher_create`'s own argument checks.** I
  instrumented all seven `INVALID_ARGUMENT` returns in that function, and none
  fired. `-3` here is `transport_error_to_cpp_ret` mapping a BACKEND
  `TransportError::{InvalidArgument, InvalidConfig, TopicNameInvalid}`.
* **It is not reached by the census.** A census run sets up every tier on the
  boot executor, in tier order, and spawns no tier task. It records all four
  nodes every time. The failure needs concurrent setup.
* **It is probably not a fixed capacity.** A pool sized one short would lose
  the same number of tiers on every run. The number varies, from 0 to 2.

## Shape

The C++ native tier runner (`nros-cpp` `native_tier_trampoline`) and the Rust
one (`nros-board-linux`) both open one borrowed executor per tier over the
boot executor's session, each on its own platform task. Then they run that
tier's `setup` concurrently. Publisher creation on the shared session is
therefore concurrent. It is the most likely place for the race: the backend
shim's per-session publisher and liveliness bookkeeping, which a single
executor never exercises concurrently. Not yet proven: the next step is to
serialize the tier setups (or the shim's create path) and see the losses stop.

## Why it matters

A lost tier is reported only on stderr, and the image keeps running with the
remaining tiers. For the derived-tiers image that is the island's shape: a
safety function silently absent at boot. phase-463 W7's profile-vs-census
check (`scripts/check-profile-against-census.py`, P1) catches it from the
outside. That is how it was found.

## Resolution (2026-10-07)

It was two defects. One of them was the losses, and it was not a race.

### 1. The losses: a pool sized for one `/diagnostics` reporter

**Not a race.** The losses reproduced on every boot, against a private
`rmw_zenohd`. The build was `origin/main` as of the "drop two doc links to the
retired aliases" commit (phase-482 W6), which already carried issue 1711's
slot-claim fix and predates issue 1729's:

| image | runs | ok | lost tier(s) | crash |
| --- | --- | --- | --- | --- |
| `derived-tiers-cpp` `native` (6 s) | 24 | 0 | 24 (18 lost 2 tiers, 6 lost 3) | 0 |
| `realtime-rust` `native_derived` (3 s) | 24 | 0 | 24 (the control tier, every time) | 0 |

The `-3` is `TransportError::InvalidConfig`, which is zpico's `ZPICO_ERR_FULL`.
gdb at `zpico_declare_publisher_ex` showed the publisher table has **5** slots.
The C++ log says so as well: `contract monitors installed, but the /diagnostics
publisher could not be created (InvalidConfig)`.

Each tier's executor installs its own contract-monitor rows, and each one with
rows arms its own `/diagnostics` reporter (`nros::contract::arm_reporter`).
That is one publisher per tier. The C++ image creates 4 node publishers + 4
reporters = 8. The derived `ZPICO_MAX_PUBLISHERS` was 4 + **1**, because
`EntityInventory::contract_reporters` counted `self.tiers.max(1)`, and `tiers`
is the AUTHORED tier count (`execution.tiers.len()`). A bringup that authors no
tiers derives one per node (`derive_tiers_from_contracts`), so the count was 0,
then 1.

Which tiers lost depended on which publishers reached the pool first. A
reporter that loses is logged and is not fatal. A node publisher that loses
fails its tier's setup. That is why the count varied (0-3) and moved with
timing, and why it looked like a race. The Rust runner already serializes its
setups (issue 0447), so its order is fixed and it lost the same tier every
time: boot tier `derived-telem_node` (reporter + `/telem` = 2), then the
control tier's reporter takes slot 3, and `/ctrl` finds the table full.

**Fixed by issue 1729**, which landed on `main` while this was being measured.
It reached the same miscount from the Zephyr side (`realtime-rust`'s
`derived_bringup` on native_sim). `contract_reporters` is now
`executor_bound().min(rows + ages).max(1)`, where `executor_bound` is
`max(authored tiers, components)`, the same term the scheduling-context table
uses. This branch had written an equivalent bound, one reporter per
contracted node. It was dropped at rebase in favour of 1729's one spelling;
1729's own unit test covers the count. The native numbers below were re-measured
on the rebased tree, which carries 1729's count. Every road shares the
derivation, so the native images are fixed by it as well. They gain a
regression test here (below), since 1729's E2E proof was the Zephyr cell.

### 2. Concurrent tier setups on one session: a real race, and the heap corruption

The native C++ runner (`nros_board_native_run_tiers_in`) spawned every
non-boot tier at once and let their setups run concurrently, through `&mut`
aliases of ONE backend session. Every RTOS runner serializes by chaining its
spawns (issue #144), and the Linux Rust runner holds a lock (issue 0447). The
NuttX Rust runner ran the boot tier first and then spawned the rest at once,
so with three tiers or more two spawned setups overlapped.

With the pool fixed, so that all four C++ tiers really do set up at once,
helgrind (`valgrind --tool=helgrind`) reports 14 races on zenoh-pico's declare
path inside tier setup, between tier threads #4 and #5:

* `_z_get_entity_id` (`resource.c:56`, `zn->_entity_id++`), from
  `z_declare_publisher` on one tier and `_z_write_filter_create` on another;
* `_z_slist_push_back` under `_z_cache_declaration` (`session.c:305`), from
  `z_declare_publisher` on one tier and `_z_declare_liveliness_token` on
  another. That is an unlocked linked-list push.

The second one is heap corruption by construction. It is the same allocation
site (`_z_slist_new*` under a tier's `z_declare_*`) as the one gdb-caught
`malloc(): unaligned tcache chunk` above. It is rare: 0 crashes in 24 plain
boots and 40 ASan boots of the pool-fixed C++ image without the gate. So no
probability can be put on it from these runs. The 4/24 crashes measured above
predate issue 1711's slot-claim fix, which closed the larger, shim-side half.

**Fix.** `nros_node::executor::TierSetupGate` (re-exported as
`nros::TierSetupGate`) is a `no_std`, allocation-free gate (one atomic flag,
backoff through `nros_platform_sleep_us`). Every runner that spawns its tiers
at once holds it across each tier's open + `setup`:

* the native C++ runner (`nros-cpp` `native_tier_trampoline`);
* the NuttX Rust runner (`nros-board-nuttx` `nuttx_tier_trampoline`, released
  in `nuttx_run_one_tier` before it spins);
* the Linux Rust runner, whose `std::sync::Mutex` it replaces, so that there
  is ONE spelling.

The chained runners (FreeRTOS C and Rust, Zephyr C and Rust, NuttX C, ThreadX
Rust) already serialize. They were checked by reading and left alone.

The gate does not cover two declares made after setup, at run time, on
different threads. zenoh-pico's declare path is unsynchronized whoever calls
it. That is issue 1749, fixed in the same change by a per-session recursive
declare lock in `zpico.c`, below every runner. It also measured what the gap
cost on `origin/main`: issue 1711's own regression test SIGSEGVed in 3 of 100
runs, inside the undeclare half of the same list.

The gate stays even with that lock in place. The shim lock covers zenoh-pico's
state. The gate covers the Rust-side session state the tiers reach through
`&mut` aliases, such as `ZenohSession::ensure_node_liveliness`'s per-node
table and the primary-token drop in
`drop_primary_node_liveliness_if_superseded`.

### What it was not

* **Not the census or profile hooks (PR #1786).** The profile is armed only by
  `$NROS_PROFILE_OUT` and keys its cursors per thread. The census runs every
  setup on the boot executor and spawns no tier. Neither is armed in the
  failing boots above. Helgrind's reports on their spin mutexes are Rust
  atomics it cannot model; they are not shared state left unlocked.
* **Not the executor backing (issue 1571).** Each tier opens over its own
  `.bss` block. The failing create is in the RMW's pool, not in executor
  storage.

### Measured after (same harness, same router)

"Count only" is this branch's bound before rebase. It derives the same pool
as issue 1729's for both images (8 and 4 publishers). "Both" is the rebased
tree: 1729's count plus the gate.

| image | runs | ok | lost | crash |
| --- | --- | --- | --- | --- |
| `derived-tiers-cpp` `native`, count only | 24 | 24 | 0 | 0 |
| `realtime-rust` `native_derived`, count only | 24 | 24 | 0 | 0 |
| `derived-tiers-cpp` `native`, both | 24 | 24 | 0 | 0 |
| `realtime-rust` `native_derived`, both | 24 | 24 | 0 | 0 |
| `derived-tiers-cpp` `native`, both, ASan | 20 | 20 | 0 | 0 |

phase-463 W7's join (`scripts/check-profile-against-census.py`, 60 s
profiled run) now accepts both images:

* `derived-tiers-cpp`: 4 census rows <-> 4 profile rows, outputs equal the
  contract's;
* `realtime-rust` `native_derived`: 2 <-> 2.

### Regression tests

* **`realtime_tiers_e2e`**, two new native rows over two new fixtures
  (`workspace-cpp-native-derived-tiers`,
  `workspace-rust-native-realtime-derived`):
  * `native/cpp-derived` (`ConsoleTicks`, now with a native arm) fails on any
    `setup FAILED` line and needs every node's `tick=1`;
  * `native/rust-derived` is the counter proof, which a lost control tier
    fails with `/ctrl` silent.

  The pre-1729 `origin/main` binaries were copied into the fixture paths.
  Against them both rows failed 6 of 6 runs. Against the fixed binaries they
  passed 6 of 6. This is deterministic, not probabilistic, because the defect
  it pins is a count.
* The count's unit test is issue 1729's
  `an_untiered_multi_node_contract_counts_a_reporter_per_derivable_tier`.
* `executor::backing::tests::tier_setup_gate_admits_one_holder_at_a_time`:
  eight threads, zero overlap. Its negative control, a gate that admits
  everyone, fails it.
