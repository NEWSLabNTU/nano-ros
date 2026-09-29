---
id: 1551
title: "The Zephyr derived-tiers-cpp image runs out of its 64 KiB platform heap
  in tier 1 of 4, and the failed Rust Box::new then SEGVs through the 0589
  stdio recursion instead of reporting anything"
status: open
type: bug
area: zephyr, memory
severity: high
found: 2026-09-28
related: [issue-1508, issue-1537, issue-1426, issue-1425, issue-0589, issue-1566, phase-459]
---

# The Zephyr derived-tiers-cpp image runs out of heap declaring entities, then SEGVs

The `native_sim/native/64` image of `examples/workspaces/derived-tiers-cpp`
(the phase-459 W0 fixture's Zephyr entry, C++, zenoh) dies at boot in **every
variant built**. It runs its platform heap dry while tier 1 of 4 declares its
first entity, and the process then dies with SIGSEGV in `z_impl_k_mutex_lock`.
The two 30 Hz tiers are never created.

**This fixture has no `fixtures.toml` row, so no lane has ever booted it.** That
nobody reported this says nothing about whether the image works. The agent
fixing issue 1508 found it while reading tier priorities under gdb.

There are two defects here, and they are separate bugs:

1. **Sizing (configuration).** The image needs roughly twice the arena it gets.
   Each tier takes a 23,928-byte executor out of the same 64 KiB arena, and
   nothing at build time adds that up.
2. **Failure mode (code).** When an allocation fails, the result is an anonymous
   SIGSEGV instead of an error. An infallible `Box::new` in the RMW C adapter
   calls std's OOM hook. The hook `eprintln!`s, and on native_sim that write is
   the issue-0589 `zvfs_write` recursion. The SEGV is a **stack overflow**, not a
   corrupted or half-built mutex.

## Evidence base

- **Images.** The three images built for issue 1508 are
  `…/agent-a069fc07529ca0b1f/tmp/1508-zbuild-{default,np32,read100}`: Zephyr 3.7,
  the `nano-ros-workspace` west tree, and the #1395 worktree as the module. They
  were used read-only, and none was rebuilt.
- **Router.** `rmw_zenohd`, resolved with `scripts/dev/zenohd.sh`
  (`nros_zenohd_bin` — the path is whatever the resolver's three steps pick on
  this host, so it is deliberately not written here; issues 0653/0654) and
  started with `nros_router_exec tcp/127.0.0.1:7447`.
- **Run command.** `zephyr.exe --seed=1551 --stop_at=8`.
- **What was re-run.** The run was repeated plain and under gdb. The plain
  console output is **byte-identical** across `default`, `np32` and `read100`
  (same md5), and every run exits 139. Those variants only change
  `CONFIG_NUM_PREEMPT_PRIORITIES` and `CONFIG_NROS_ZENOH_READ_PRIORITY`, so this
  is expected.

## The failure, verbatim (plain run, `default`)

```
WARNING: Using a test - not safe - entropy source
*** Booting Zephyr OS build v3.7.0 ***
[nros] tier task entered
nros: HEAP EXHAUSTED: request 48 bytes, arena 66048 bytes, caller 0x4266f9
      (addr2line -f -e zephyr.elf 0x4266f9 to name it; raise CONFIG_NROS_ZEPHYR_HEAP_SIZE / NROS_ZEPHYR_HEAP_SIZE only once you know what asked)
nros: HEAP EXHAUSTED: request 320 bytes, arena 66048 bytes, caller 0x426436
      (…)
nros: HEAP EXHAUSTED: request 320 bytes, arena 66048 bytes, caller 0x426436
      (…)
nros: HEAP EXHAUSTED: request 48 bytes, arena 66048 bytes, caller 0x4294c0
      (…)
zpico: z_liveliness_declare_token failed: -78 for '@ros2_lv/0/c40c0137f1e48f6cfee1f90b51ffd9aa/0/0/NN/%/%/mrm_handler'
nros: HEAP EXHAUSTED: request 159 bytes, arena 66048 bytes, caller 0x426c9a
      (…)
zpico: z_liveliness_declare_token failed: -78 for '@ros2_lv/0/…/0/5/MP/%/%/mrm_handler/%system%fail_safe%mrm_state/std_msgs::msg::dds_::Int32_/TypeHashNotSupported/1:2:1,10:,:,:,,'
nros: HEAP EXHAUSTED: request 472 bytes, arena 66048 bytes, caller 0x4a6397
      (…)
timeout: the monitored command dumped core
```

Under gdb:

```
Thread 11 "zephyr.exe" received signal SIGSEGV, Segmentation fault.
z_impl_k_mutex_lock (mutex=mutex@entry=0x4f4450 <fdtable+112>, timeout=...) at zephyr/kernel/mutex.c:115
```

`-78` is `_Z_ERR_SYSTEM_OUT_OF_MEMORY` (zenoh-pico `utils/result.h:84`).

## Which heap is 66,048 bytes (measured)

The arena is **not** `CONFIG_HEAP_MEM_POOL_SIZE`, and it is **not** picolibc's
`CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE`.

It is the rlsf `FreeListHeap` in `nros-platform/src/zephyr_heap.rs`. That arena
sits behind `nros_platform_alloc` (`nros-platform-zephyr/src/platform.c`), and
both zenoh-pico's `z_malloc` and Rust's `__rust_alloc` go through it. It has
done so since phase-391 W3. Every failing allocation's backtrace passes through
`nros_platform_alloc`, and that includes the Rust one:
`nros_platform::global_allocator` → `nros_platform_cffi` → `nros_platform_alloc`.

Its size is `CONFIG_NROS_ZEPHYR_HEAP_SIZE`, which is forwarded to cargo by
`_nros_resolve_knob` in `zephyr/cmake/nros_cargo_build.cmake`:

- `.config` holds `CONFIG_NROS_ZEPHYR_HEAP_SIZE=65536`. That is the **Kconfig
  default**, because none of the four merged fragments sets it. The build log's
  `Merged configuration` lines are `prj.conf`, `prj-zenoh.conf`,
  `zephyr/native-sim-nsos.conf` and `extra_kconfig_options.conf`.
- The printed `66048` is `capacity()` = `N + SLAB_REGION_SIZE`, which is
  65,536 + 8 × 64 (`zpico-alloc/src/lib.rs:483`).
- **No example in the tree sets `CONFIG_NROS_ZEPHYR_HEAP_SIZE`.** It appears
  only in comments, in 12 `prj-xrce.conf` files. So every Zephyr image runs on
  the 64 KiB default.

The image's `prj-zenoh.conf` does size heaps, but they are the two that the
failing allocations did not come from:

```
CONFIG_HEAP_MEM_POOL_SIZE=262144
CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=1048576
```

That is 1.28 MiB set aside, while the arena that actually ran out stayed at
64 KiB. `realtime-cpp/src/zephyr_entry/prj-zenoh.conf` carries the same two
lines. Whether anything else in the image allocates from those two heaps was
not measured.

**CLAUDE.md is stale on this point.** Its pitfall-index line reads: "Zephyr Rust
allocator is picolibc `malloc` — size `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` …
NOT `CONFIG_HEAP_MEM_POOL_SIZE`". That line predates phase-391 W3 and now points
a reader at the wrong knob. It should name `CONFIG_NROS_ZEPHYR_HEAP_SIZE`. It was
not edited here, because this is a filing.

## What allocates, and how far it gets (measured)

The run used a gdb breakpoint on `nros_platform_alloc` that printed the request
size, `nros_zephyr_heap_used()` before the call, and a short backtrace. It
recorded 157 allocations up to the fatal one. `used` is the value before each
call:

| Phase | used before → after | Cost |
| --- | --- | --- |
| Boot executor storage (`nros_board_zephyr_run_tiers_ns`, 1 × `NROS_CPP_EXECUTOR_STORAGE_SIZE` = 23,928) | 0 → 23,952 | 23,952 |
| Session open (zenoh config, 6 × 2,048-byte link/transport buffers, session, session liveliness, `Box<ZenohSession>` 496) | 23,952 → 34,272 | 10,320 |
| Boot executor `assemble` (Arcs, `NodeWake`) | 34,272 → 34,512 | 240 |
| **Tier 0** `MrmComfortableStopOperator`: 1 publisher + 1 timer, including the node `NN` token, the entity `MP` token, write-filter interest and `Box<ZenohPublisher>` 472. The timer allocated nothing here. | 34,512 → 38,272 | **3,760** |
| **Tier 1 spawn** (`zephyr_spawn_next_tier`: executor storage 23,928 + ctx 104 + `assemble`) | 38,272 → 62,528 | **24,256** |
| **Tier 1** `MrmHandler` publisher | 62,528 → exhausted at 64,256 | — |

So:

- **1 of 4 tiers** fully declared its entities.
- Tier 1 opened its executor and then failed on its first publisher.
- Tiers 2 and 3 (the two 30 Hz tiers, `derived-mrm_emergency_stop_operator` and
  `derived-stop_mode_operator`) were never spawned.

The table is `__nros_tiers[4]` in the generated entry TU, and the build log says
`8 entities, 4 executor callback slots`.

The failing request order was: zenoh-pico 48, 320, 320 and 48 for the node
token, then 159 for the entity token, then Rust 472 for `Box<ZenohPublisher>`.
The first failure happened at `used = 64,256` against 66,048 of capacity, so
some fragmentation had already set in.

**Inferred (arithmetic, not run).** The full image would need about
`23,952 + 10,320 + 240 + 3 × 24,256 + 4 × 3,760 ≈ 122 KB`, which is about 1.9×
the arena. Most of that is executor storage: **4 × 23,928 = 95,712 bytes, which
on its own already exceeds 66,048.** No entity sizing can make this image fit
the default.

## Defect 1: nothing prices N tiers of executor storage against the arena

`zephyr_run_tiers.c` takes `NROS_ZEPHYR_EXECUTOR_STORAGE_BYTES` out of the
platform heap:

- once for the boot tier (`:633`);
- once more for **every** further tier (`:477`).

That size is `NROS_CPP_EXECUTOR_STORAGE_SIZE`, the per-build derived value.
The one build-time check that relates an executor to this heap is the arena gate
at `nros_cargo_build.cmake:1025-1066` (`arena + 24576 <= NROS_ZEPHYR_HEAP_SIZE`),
and it misses this case in two ways:

- it runs only when `NROS_EXECUTOR_ARENA_SIZE` is stated explicitly, and here the
  value is derived;
- it prices **one** executor.

Nothing multiplies the storage by the tier count. The build knows all three
numbers: the tier table, the storage size and the heap size. It still produced
an image that cannot boot and said nothing.

### Scope (read only, nothing built)

- The laned sibling `realtime-cpp` (`workspace-zephyr-cpp-realtime`, two Zephyr
  tiers) has the same conf shape. An **old** build of it
  (`nano-ros-workspace/build-ws-cpp-realtime-entry-zenoh`, dated 2026-09-10)
  records `NROS_CPP_EXECUTOR_STORAGE_SIZE 89992`. Run with a router on its
  locator, it prints `HEAP EXHAUSTED: request 89992 bytes, arena 66048 bytes` as
  its **first** allocation, before any tier exists. It then idles, reaches
  `--stop_at`, and **exits 0**.
- That build is a museum binary, so this says nothing about HEAD. It does show
  that the same shape — per-tier storage from the fixed 64 KiB default — has
  already produced an image that cannot start, and one that fails **silently**
  when the NULL is handled.
- **Not measured:** whether a fresh build of `realtime-cpp` or `realtime-c` fits.
  The derived storage differs per image (23,928 here, 89,992 there). The
  `realtime_tiers_e2e` `zephyr_cpp`/`zephyr_c` cells are where that answer
  belongs.
- **Any** Zephyr multi-tier C/C++ image is exposed, because the storage size
  scales with the entity inventory and the arena does not. Single-executor
  images pay the storage once.

### Direction (not implemented)

- Choose one of these:
  - price the arena at configure time, where all three inputs exist:
    `(1 + n_tiers_beyond_boot) × storage + session_floor + per-entity`, and
    refuse to build below it (same shape as the existing gate);
  - or derive `NROS_ZEPHYR_HEAP_SIZE` from that sum when Kconfig leaves it at
    the default.
- Alternatively, move tier executor storage out of the heap into
  linker-visible `.bss`, as the Rust arm did (`EXECUTOR_BACKING`, RFC-0002
  § 4.4b). Then `mem-report` can see it, and it cannot fail at run time. The
  CLAUDE.md executor-arena entry says the same move applies here: subtract what
  it removes from the heap, and never report the `.bss` growth alone.
- Then give the fixture a `fixtures.toml` row and a `matrix::CELLS` cell, so a
  lane boots it.
- Drop or justify the dead-looking 256 KiB / 1 MiB heap lines in both
  `prj-zenoh.conf` files, and fix the CLAUDE.md line above.

## Defect 2: an allocation failure becomes a stack-overflow SEGV (measured)

Full backtrace of the SEGV (outermost frames; the middle is ~104,730 copies of
one frame):

```
#0       z_impl_k_mutex_lock (mutex=0x4f4450 <fdtable+112>) at kernel/mutex.c:115
#1       k_mutex_lock
#2       zvfs_write (fd=1, buf=0x4c0972, sz=21) at lib/os/fdtable.c:339
#3 …     zvfs_write (fd=1, buf=0x4c0972, sz=21) at lib/os/fdtable.c:340   (repeats)
#104738  zvfs_write (fd=2, buf=0x4c0972, sz=21) at lib/os/fdtable.c:340
#104739  std::sys::fd::unix::FileDesc::write
#104741  std::io::Write::write_all<Stderr>
#104746  std::alloc::default_alloc_error_hook          (library/std/src/rt.rs:44)
#104749  std::alloc::rust_oom
#104752  alloc::alloc::handle_alloc_error
#104754  Box::<ZenohPublisher>::new
#104755  nros_rmw_cffi::rust_adapter::create_publisher_trampoline   (rust_adapter.rs:829)
#104757  nros_cpp::publisher::nros_cpp_publisher_create           (publisher.rs:161)
#104761  mrm_handler_pkg::MrmHandler::MrmHandler                   (MrmHandler.cpp:27)
#104762  __nros_entry_setup_tier_1
#104764  zephyr_tier_task                                          (zephyr_run_tiers.c:415)
```

At the fault:

- `x/s 0x4c0972` gives `"memory allocation of \300\016 bytes failed\n"`. That is
  std's OOM message, still unformatted.
- `p *mutex` shows `owner = nros_tier_threads`, `lock_count = 104735`.

This is **exactly issue 0589's recursion**: `stdinout_write_vmeth` re-enters
`zvfs_write`, the recursive `k_mutex` never deadlocks, and the stack runs out.
The only difference is the producer. 0589 was resolved by routing **our**
`std::eprintln!` sites through `nros_log`, and `check-no-std-stdio` gates our
sources. This write comes from **inside libstd** (`default_alloc_error_hook`),
which neither the fix nor the gate can see. The class is "any libstd stdio on a
native_sim image", and std's OOM hook is a member nobody swept.

So the answer to "unchecked return, or partially constructed object?" is
**neither**:

- zenoh-pico's first four NULLs **were** checked. They came back as `-78`, and
  `ensure_node_liveliness` treats a lost token as soft by design: it logs, and
  the node is missing from `ros2 node list`.
- The one allocation that was **not** fallible is Rust's `Box::new(pub_handle)`
  at `rust_adapter.rs:829`. An infallible allocation cannot return NULL. It
  calls `handle_alloc_error`, and in a `std` build that means "print, then
  abort".
- On native_sim the print kills the process before the abort does, so the one
  line that would have named the failure never appears. The same infallible
  `Box::new` shape is at `rust_adapter.rs:737`, `:933`, `:1150` and `:1308`
  (session, subscriber, service server, service client).

Also measured: `CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL` is **off** in this image,
because its default is `y if NROS_BOOT_REPORT` and the image does not set
`NROS_BOOT_REPORT`. So the first `HEAP EXHAUSTED` returned NULL rather than
halting through `nros_platform_panic`. With the knob on, the image would have
stopped cleanly at the first 48-byte failure and never reached the `Box::new`.
That is a mitigation for this image, not a fix for the class. This is the knob
issue 1425 concerns.

**Inferred, not measured:** on a real (non-native_sim) Zephyr target, the Rust
side has no libstd, so `handle_alloc_error` goes to that target's panic or
alloc-error path rather than to `zvfs_write`. The crash would look different,
but it would still be a crash on a path whose C caller expects a return code.

### Direction (not implemented)

- Make the RMW adapter's handle boxes fallible:
  - allocate through `alloc::alloc::alloc(Layout)`, check for NULL and return
    `NROS_RMW_RET_BAD_ALLOC`;
  - or add a `try_box` helper, because `Box::try_new` is unstable.
- Sweep every `Box::new`, `Arc::new` and `Vec` growth on an entity-creation path
  that a C caller reaches (`nros-node` `assemble` and `NodeWake` are in the trace
  too), so an exhausted heap becomes a returned error that the C++ API already
  has a code for.
- Separately, route std's alloc-error output on Zephyr away from the fdtable.
  `std::alloc::set_alloc_error_hook` is unstable. The practical lever is the
  fallible adapter above, plus `NROS_HEAP_EXHAUSTION_IS_FATAL` so the platform
  heap halts first.
- Extend 0589's class note to say that libstd itself is a producer.

## For the Rust `nros::main!` road + Zephyr boot-report work (the `related:` issue filed on PR #1395)

- A Rust Zephyr native_sim image whose heap is exhausted at any infallible
  allocation will die the same way: std OOM hook, then `zvfs_write` recursion,
  then SIGSEGV, with no message.
- Size `CONFIG_NROS_ZEPHYR_HEAP_SIZE` (not the two heaps above) before reading
  anything from such a boot.
- With `CONFIG_NROS_BOOT_REPORT=y`, `NROS_HEAP_EXHAUSTION_IS_FATAL` defaults on,
  and the halt lands before the recursion can happen.

## Not measured

- The full demand of the image with a large enough arena. No rebuild was made,
  so the ~122 KB figure is arithmetic.
- Whether the tier-1 fragmentation, not the total, decided the exact failing
  request.
- Fresh builds of the laned `realtime-cpp` / `realtime-c` Zephyr entries.
- Whether anything allocates from `HEAP_MEM_POOL` or picolibc's arena in this
  image.
- Why the `nros_log` "node declared no token" error line from
  `ensure_node_liveliness` does not appear. The likely cause is that deferred
  `CONFIG_LOG` never flushed before the crash, but that was not checked.

## Acceptance

- The derived-tiers-cpp Zephyr image boots all four tiers under a router, with a
  lane that boots it: a `fixtures.toml` row plus a cell.
- A build whose tiers cannot fit the platform heap fails at configure time with
  the numbers, or the storage is no longer heap-allocated.
- A failed allocation on an entity-creation path returns an error code to the
  C/C++ caller. Negative control: `CONFIG_NROS_ZEPHYR_HEAP_SIZE` too small gives
  a reported error, not SIGSEGV.

## Resolution — defect 1 (sizing)

**Status: defect 1 fixed. The issue stays OPEN.** Defect 2 (an allocation
failure turns into a stack-overflow SEGV) was being fixed separately in
`packages/rmw/**/rust_adapter.rs` when this was written; it has since landed —
see "Resolution — defect 2 only" below, which also lists what remains open
under it. This issue closes when everything both sections leave open is done.

### Direction chosen: move tier storage into `.bss`, sized by the entry

Four directions were weighed (the issue's three, plus the conf lines):

- **Check the total at configure time.** This cannot be done honestly.
  `NROS_CPP_EXECUTOR_STORAGE_SIZE` is produced by nros-cpp's `build.rs`, at
  CARGO BUILD time, after configure has finished. A configure-time price would
  need a second, guessed copy of that number, and that is the 0088/0245
  sizes-header class.
- **Derive `NROS_ZEPHYR_HEAP_SIZE` from the total.** This has the same
  timing problem. It also keeps the largest term, `n_tiers × storage`, in a
  pool that `mem-report` cannot see, and it can still fail at run time.
- **Move the storage into static memory.** This is the chosen direction. The
  generated entry is the one place that knows both the tier count and the
  per-build storage size, so it emits the storage itself:
  `static uint64_t __nros_tier_executor_storage[n_tiers][(NROS_CPP_EXECUTOR_STORAGE_SIZE + 7) / 8]`.
  It passes that storage to `nros_board_zephyr_run_tiers_in(…, storage, stride)`.
  The storage is then an object the linker places. On a RAM-bounded board, an
  image that cannot hold its tiers fails at **link**. `mem-report` names it
  too, e.g. `__nros_tier_executor_storage 181,056` on realtime-cpp. No
  tier-multiplied demand is left on the heap, so there is nothing there to
  mis-size silently.
- **The two misdirected conf lines are fixed as well.** See below.

This is a **move, not a saving**, per the CLAUDE.md executor-arena rule. The
bytes the heap would have needed become `.bss`:

- realtime-cpp: 2 × 90,528 = 181,056 bytes;
- derived-tiers-cpp: 4 × 23,976 = 95,904 bytes.

What the heap still holds is the session plus the per-entity cost, and that is
measured below. The costs of the choice:

- **RAM.** `.bss` is reserved whether or not a tier ever spawns. A heap block
  was also taken for the image's whole lifetime, so the steady state is
  unchanged.
- **Linker visibility.** Gained.
- **`mem-report` visibility.** Gained. The heap blocks had no symbol.

Mechanics:

- `CAbiRunners::run_tiers_takes_storage` sits beside the runner name in
  `nros-entry-lower`. It is true for Zephyr only, and both entry packs read it.
- The runner refuses a stride smaller than this build's executor size, and
  refuses one that is not 8-byte aligned, instead of overrunning it.
- `nros_board_zephyr_run_tiers_ns` stays for entries generated before this
  change. It now takes all of the tiers' storage in ONE heap block up front and
  prints the numbers, rather than failing part-way down the spawn chain.
- FreeRTOS and NuttX `run_tiers` have the same per-tier heap shape against
  their own heaps. They are **not** changed (`run_tiers_takes_storage: false`).
  Issue 1568 later moved them (and the C single-executor runner) to the same
  shape, renamed the flag `CAbiRunners::takes_executor_storage`, and moved the
  size and the refusal into the linked library
  (`nros_cpp_executor_storage_size` / `_check`).

### Reproduction on `main` before the change (native_sim/native/64, `rmw_zenohd`)

Measured on `main` at 5519d5caf.

**derived-tiers-cpp**, after `nros sync` of the workspace. This is the
issue's failure: the image derives `__nros_tiers[4]` and has 23,976 B of
storage per tier.

```
[nros] tier task entered
nros: HEAP EXHAUSTED: request 66 bytes, arena 66048 bytes, caller 0x4276a6
zpico: z_liveliness_declare_token failed: -78 for '@ros2_lv/0/…/0/0/NN/%/%/mrm_handler'
nros: HEAP EXHAUSTED: request 24 bytes, arena 66048 bytes, caller 0x4344cb
zpico: z_liveliness_declare_token failed: -78 for '@ros2_lv/0/…/0/5/MP/%/%/mrm_handler/%system%fail_safe%mrm_state/…'
nros: HEAP EXHAUSTED: request 472 bytes, arena 66048 bytes, caller 0x4b2cd7   (alloc::boxed::box_new_uninit)
timeout: the monitored command dumped core                                    (exit 139)
```

**Sync is load-bearing.** Without `nros sync`, the configure names a
SystemModel that does not exist (`src/demo_bringup/config/system_model.yaml`).
The derivation then silently returns nothing, and the image runs
single-executor and boots. Its one executor's storage is already static
(`rclcpp::Node::GlobalStorageHolder`), so that boot says nothing about this
defect. A report of this image booting all four tiers on `main` at the 64 KiB
default does not match this measurement. If you see one, check which module
tree it was built against.

**The two LANED tiered images** fail at their **first** allocation, with no
sync involved:

```
realtime-cpp:  nros: HEAP EXHAUSTED: request 90528 bytes, arena 66048 bytes, caller 0x424f65
               (addr2line: nros_board_zephyr_run_tiers_ns, zephyr_run_tiers.c:637)
               … idles to --stop_at, exit 0
realtime-c:    nros: HEAP EXHAUSTED: request 90528 bytes, arena 66048 bytes, caller 0x424de8
```

Neither of them ever runs a tier.

### After

| Image | Tiers | Result | Platform heap peak / capacity |
| --- | --- | --- | --- |
| realtime-cpp | 2 | both tiers tick (`[ctrl]`, `[telem]`), EDF deadline set | 18,896 / 66,048 |
| realtime-c | 2 | both tiers tick, once issue 1566 was fixed (below) | 18,896 / 66,048 |
| derived-tiers-cpp (after `nros sync`) | 4 derived | all 4 tiers up | 29,360 / 66,048 |

For derived-tiers-cpp, the generated table is `__nros_tiers[4]` and the storage
is `__nros_tier_executor_storage[4][…]` (0x176a0 = 95,904 B `.bss`). An 8 s run
printed `[nros] tier task entered` ×3, and all four nodes ticked: the two 30 Hz
nodes 239 and 240 times, the two 10 Hz nodes 79 times each. A graph query while
it ran:

```
$ ros2 node list --no-daemon        $ ros2 topic list --no-daemon
/mrm_comfortable_stop_operator      /system/emergency/control_cmd
/mrm_emergency_stop_operator        /system/fail_safe/mrm_state
/mrm_handler                        /system/mrm/comfortable_stop/status
/stop_mode_operator                 /system/stop_mode/control
```

That is every tier's node and publisher declared. The issue predicted about
26 KB of heap for this image once the storage was gone (122 KB − 95,712). The
measured peak is 29,360.

Two unrelated things this run exposed:

- **realtime-c.** With the heap no longer empty, realtime-c got as far as
  tier 1's first declare and then SEGVed. This was a separate latent overrun:
  `NROS_C_PUBLISHER_STORAGE_SIZE` was a literal 560, but the create call writes
  640 bytes, and the component instance sat directly before the heap. This is
  issue **1566**, filed and fixed alongside.
- **derived-tiers-cpp configure.** Without a prior `nros sync`, this image's
  configure names `src/demo_bringup/config/system_model.yaml`, a file that
  does not exist. The derivation then returns nothing, silently, and the image
  runs single-executor. The fixture row needs to sync first; see below. This is
  not filed separately here. It is the "no model ⇒ no derivation" edge, and it
  should probably fail loud.

### The two misdirected heaps (measured before editing)

The lines were `CONFIG_HEAP_MEM_POOL_SIZE=262144` and
`CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=1048576`, in derived-tiers-cpp,
realtime-cpp and realtime-c. They were measured with gdb breakpoints on
`k_heap_aligned_alloc` and on Zephyr's common-libc `malloc`, over boot + 3 s
under a router:

- **Kernel heap: 232 B in two calls, both from Zephyr's own NSOS driver**
  (`nsos_getaddrinfo` 144 B, `nsos_socket_create` 88 B). This is the CLAUDE.md
  warning, confirmed: the kernel heap cannot be zero. It is now 64 KiB, the
  value every sibling C/C++ workspace conf states.
- **libc malloc: zero calls.** The 1 MiB line is removed, and the native_sim
  Kconfig default (16 KiB) now applies.

Together that is 1.28 MiB less static RAM in each image, and it is independent
of the move. The Rust workspaces' native_sim confs carry the same 1 MiB line.
Their allocation profile was not measured, so they are unchanged.

### Not done / not verified

- **Link-time failure on a RAM-bounded board: not demonstrated.** Every image
  above is native_sim, which has no RAM bound. The claim that an image too
  large for its board now fails at link rests on the storage being an ordinary
  `.bss` object (nm and `mem-report` both list it); no mps2/an385 build was run
  to show it.
- **The rest of the heap is still unpriced.** The session plus per-entity cost
  (18.9–29.4 KB here) is not checked at build time. An image with a
  deliberately small `CONFIG_NROS_ZEPHYR_HEAP_SIZE` still fails at boot, and
  whether that failure is reported cleanly is defect 2 and issue 1425.
- **No lane boots the fixture yet.** A lane needs a `fixtures.toml` row and a
  `matrix::CELLS` cell. Those files belong to a concurrent change, so they are
  not edited here. The row needs:
  - the west entry at `examples/workspaces/derived-tiers-cpp/src/zephyr_entry`;
  - `board native_sim/native/64`, `lang cpp`, `rmw zenoh`;
  - confs `prj.conf;prj-zenoh.conf` plus the NSOS fragment;
  - an `nros sync` of the workspace BEFORE configure, or the tiers are never
    derived.

  The cell should assert that each of the four nodes ticks (or appears in
  `ros2 node list`) under a router. `main`'s derivation already produces the
  four tiers once the model exists; PR #1356 is not required for that.

## Resolution — defect 2 only (2026-09-29)

**Defect 1 (sizing) is being fixed separately, so this issue stays `open`.**
What follows closes defect 2. An allocation failure on an entity-creation
path now returns `BAD_ALLOC` and prints a named log line. It no longer ends
in a stack-overflow SIGSEGV.

### The image changed after filing, so the reproduction forces the failure

On `main` at 5519d5caf, the default `derived-tiers-cpp` image **boots all four
tiers** at 64 KiB. Its measured heap peak is 28,352 bytes of 66,048, so the
executor storage this issue priced no longer comes from this heap.

The reproduction therefore shrinks the heap in a LOCAL fragment:
`CONFIG_NROS_ZEPHYR_HEAP_SIZE=13312`, passed as `EXTRA_CONF_FILE` and never
committed. The value comes from a gdb trace of every `nros_platform_alloc` on
the default image:

- The first `Box<ZenohPublisher>` (472 bytes) is allocation #120.
- 13,664 bytes are in use just before it.
- At 13,824 bytes of capacity, it is the first Rust allocation to fail.

Setup: Zephyr 3.7, native_sim/64, `rmw_zenohd` on `tcp/127.0.0.1:7447`,
`zephyr.exe --seed=1551 --stop_at=8`.

**Before** (the adapter as it is on `main`), verbatim. Each `HEAP EXHAUSTED`
line is followed by its `addr2line` hint, elided here:

```
*** Booting Zephyr OS build v3.7.0 ***
nros: HEAP EXHAUSTED: request 320 bytes, arena 13824 bytes, caller 0x4264d5
nros: HEAP EXHAUSTED: request 320 bytes, arena 13824 bytes, caller 0x4264d5
nros: HEAP EXHAUSTED: request 320 bytes, arena 13824 bytes, caller 0x4264d5
nros: HEAP EXHAUSTED: request 472 bytes, arena 13824 bytes, caller 0x4b1987
timeout: the monitored command dumped core
exit=139
```

Under gdb the stack is 104,775 frames deep. Read from the fault outwards:

- `zvfs_write(fd=1, sz=21)`, repeated
- `std::sys::stdio::unix::Stderr::write`
- `std::alloc::default_alloc_error_hook` (`rt.rs:44`)
- `handle_alloc_error`
- `Box::<ZenohPublisher>::new`
- `create_publisher_trampoline` (`rust_adapter.rs:838`)
- `nros_cpp_publisher_create`
- `MrmEmergencyStopOperator`
- `__nros_entry_setup`

This is the shape the issue recorded. The only difference is that it now
happens on the boot thread instead of tier 1.

**After**, with the same image and the same fragment:

```
*** Booting Zephyr OS build v3.7.0 ***
nros: HEAP EXHAUSTED: request 320 bytes, arena 13824 bytes, caller 0x426655
nros: HEAP EXHAUSTED: request 320 bytes, arena 13824 bytes, caller 0x426655
nros: HEAP EXHAUSTED: request 320 bytes, arena 13824 bytes, caller 0x426655
nros: HEAP EXHAUSTED: request 472 bytes, arena 13824 bytes, caller 0x44b720
[00:00:00.050,001] <err> nros: nros: [    0.050001] heap exhausted: could not allocate 472 bytes for a publisher handle; the publisher is NOT created (NROS_RMW_RET_BAD_ALLOC). Raise the platform heap (Zephyr: CONFIG_NROS_ZEPHYR_HEAP_SIZE)

Stopped at 8.001s
exit=0
```

The sequence after the failure:

1. The publisher create returns `BAD_ALLOC`, which maps to `NROS_CPP_RET_FULL`.
2. The component reports not-ok, so `__nros_entry_setup` returns non-zero.
3. `run_components` shuts the session down (gdb shows `nros_cpp_fini` called
   from `nros::shutdown`).

The image does not run, and it says why.

**Negative direction.** The same build dir, reconfigured back to the default
heap (65,536), boots all four tiers:

- 632–638 tick lines over 8 simulated seconds: 237 each from the two 30 Hz
  tiers and 79 each from the two 10 Hz tiers.
- No `HEAP EXHAUSTED` line.
- `Stopped at 8.000s` and exit 0 in 2 of 3 runs.

The third run hit the harness's 60 s wall-clock timeout at tick 236 of about
238, with host load around 35. It was slow, not a crash: no signal and no
error line.

One visible difference remains. Three `nros_log` lines that the pre-fix image
never printed now reach the console: `arena over-provisioned …` and two
`timer period 33000 us …` warnings. That is fix 4 below working, not a
regression.

### What was fixed

1. **One way to spell a fallible allocation.**
   `nros_rmw::fallible::{try_box, try_zeroed_bytes}` (`alloc` feature).
   `try_box` is the stable form of the unstable `Box::try_new`. It allocates
   `Layout::new::<T>()` through the global allocator and, on NULL, hands the
   value BACK so the caller can dispose of it.
2. **The five adapter sites.** Session, publisher, subscription, service and
   client in `rust_adapter.rs` now go through one `box_handle`. It logs the
   named error through `nros_log` and returns `NROS_RMW_RET_BAD_ALLOC`.
   - The backend's handle is dropped, which undeclares the entity.
   - A session that cannot be boxed is `close`d first, as
     `destroy_session_trampoline` does.
3. **The same class in `nros-node`.** These allocations now return
   `TransportError::BadAlloc` from functions that already returned `Result`:
   - the three event-closure boxes (`register_pub_event`,
     `register_sub_event_count`, `register_sub_event_liveliness`);
   - the 2 × 4 KiB parameter-service buffer pair
     (`ParamServiceBuffers::try_with_capacity`), the largest Rust allocation
     on the creation path.
4. **The log line needed a sink.** The first fixed build returned `BAD_ALLOC`
   and printed nothing.
   - Every Rust board funnel installs a `nros_log` sink by calling
     `nros_platform_cffi::log::init_default()`.
   - No C or C++ board funnel does: not `zephyr_run_tiers.c`, not
     `nros_rtos_run_components.c`, not `ZephyrBoard::run_components`.
   - So `nros_log::early` held every Rust-side record forever, until the
     image's own code made its first C log call.

   This also answers the issue's open question about the missing
   `ensure_node_liveliness` line. Deferred `CONFIG_LOG` was not the cause:
   the record was never dispatched.

   `nros_support_init_rmw`, `nros_cpp_init_rmw` and `nros_cpp_init_multi` now
   call nros-c's existing idempotent `ensure_default_sinks()` first. The sink
   is up before the runtime can raise anything.

### Can the OOM handler itself be made safe? Not on this toolchain (measured)

All three levers are nightly-only on the pinned stable rustc 1.98.1. Each is
refused with E0658 or "the option `Z` is only accepted on the nightly
compiler":

- `std::alloc::set_alloc_error_hook` (feature `alloc_error_hook`);
- `#[alloc_error_handler]` (feature `alloc_error_handler`);
- `-Zoom=panic`, which would route OOM through the stable `panic::set_hook`.

The global allocator cannot do it on the caller's behalf either.
`GlobalAlloc::alloc` cannot tell `Box::new` from `try_reserve`, so halting on
NULL there would turn every fallible allocation into a halt too. That is
exactly what `CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL` already offers, as an
image-wide policy (issue 1425).

So the fix is at the call site. The handler stays unsafe for every allocation
this sweep did not reach.

**Read from source, not built.** `stdinout_write_vmeth` recurses only under
`CONFIG_BOARD_NATIVE_POSIX`, which native_sim sets through
`NATIVE_SIM_NATIVE_POSIX_COMPAT` (default `y`). With that option off, the
picolibc arm returns `0`: a libstd stderr write becomes a silent no-op and the
abort runs (SIGABRT, still no message).

That would remove the stack overflow for the whole 0589 class, but it silences
the message instead of printing it. Flipping a board-compat Kconfig for every
native_sim image also deserves its own issue.

### Reach of the sweep, and what was excluded

The rule: **an allocation on an entity-creation path, whose enclosing function
ALREADY has an error return,** is made fallible.

Candidates came from
`git grep -nE 'Box::new|Arc::new|vec!\[|Vec::with_capacity|format!|\.to_vec\(\)'`
over `packages/rmw`, `packages/core/nros-node/src` and `packages/api/*/src`.
The zenoh backend makes no Rust allocations of its own on this path: its
handles are the adapter's boxes, and zenoh-pico's C allocations were already
checked.

Excluded, each deliberately:

- **Executor assembly.** `spin.rs` `assemble` allocates the
  `halt_flag`/`wake_flag` `Arc::new`s and `NodeWake::new`'s `vec!`: 240 bytes,
  once per executor.
  - `assemble` returns `Self`.
  - `portable_atomic_util::Arc` has no fallible constructor: `Arc::try_new` is
    unstable, and `Arc::from(Box)` re-allocates infallibly.
  - Making it fallible is an executor-API change.
- **`ensure_parameter_store` / `new_param_state` / `leak_parameter_storage`.**
  Infallible signatures, for the same reason.
- **The Rust `nros::main!` road.** `node_runtime.rs`'s
  `Arc::new(ComponentCell)`, `from_executor`'s `vec!` and
  `__private_node_state_into_raw`: each is either an `Arc` or has no error
  return to pass a failure through.
- **Service-callback-time allocations.** The
  `Box::new(Response::default())` calls in `parameter_services.rs` and
  `lifecycle_services.rs` are not on the creation path. A failure there needs a
  policy for what to reply, which is a different contract.
- **`lending` borrow boxes** (`subscription.rs` in nros-c and nros-cpp).
  They are per-message, and no shipped build enables `lending` (issue 0814).
- **Host-only paths** (`NativeTierCtx`, metadata-mode `format!`). These never
  run on an embedded image.
- **`nros-log`.** It needs no exclusion: it has no `Vec` or `String` growth,
  because it formats into a fixed `FormatBuffer`. That is why reporting an
  exhausted heap does not itself need the heap.

### Gate: declined

The question a gate would have to answer is "does a C caller with an error
return reach this allocation?". That is call-graph reachability, which a text
scan cannot see.

`Box::new` is also only one of at least six spellings (`vec!`, `Vec::push`,
`String` growth, `format!`, `Arc::new`, `collect`). A grep gate would have to
allowlist the hundreds of legitimate infallible allocations in tests, host
tools and std-only code: an allowlist in disguise.

The regression guard for the fixed sites is behavioural instead:
`packages/rmw/cffi/tests/adapter_oom.rs` installs a global allocator that
refuses one request size. It asserts that every create trampoline returns
`BAD_ALLOC` and drops the backend's handle. It is mutation-tested: against the
pre-fix adapter, the test binary dies with
`memory allocation of 4091 bytes failed` and SIGABRT.

### Still open under defect 2

- Every excluded site above still reaches libstd's OOM hook. On native_sim
  that is still the 0589 recursion.
- `nros::detail::report_component_failure` does nothing on a freestanding
  Zephyr C++ image:
  - `NROS_ERROR` falls to the `((void)…)` arm of `NROS_LOG_SINK`;
  - the `fprintf` arm needs `__STDC_HOSTED__`.

  So the component-level "FAILED at …" line that the generated entry tries to
  print never appears, and the adapter's line above is the only diagnostic.
  This is not fixed here.
