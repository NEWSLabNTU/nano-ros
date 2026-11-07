# Measure static memory

<!-- type: how-to · level: 3 -->

**Needs first:** [Size an image from its contract](size-an-image-from-its-contract.md)

The [Static Pool Inventory](../../reference/static-pool-inventory.md) tells you
which knobs exist and what each one costs: a byte figure at its default where
the pool is a plain product of knobs, and otherwise *where* its bytes go — a
named static, the executor's storage, a heap — and why no figure can be stated
without building. It cannot tell you what *your* image costs — your knobs
differ, your backend differs, and several pools are sized by a `sizeof` that no
comment can see.

For that, measure the image you built. Two ways:

- **Without a nano-ros checkout**, read the ELF with the LLVM tools rustup
  ships (`rustup component add llvm-tools` once). They read an image for any
  target:

  ```text
  bin="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin"
  "$bin/llvm-size" -A build/.../your-image      # RAM = .data + .bss (+ .uninit)
  "$bin/llvm-nm" --size-sort -S -C --radix=d build/.../your-image | tail -20
  ```

  `llvm-size -A` gives the section totals; `llvm-nm --size-sort` lists the
  largest statics last, each with its size in bytes.
- **With a checkout**, the memory report below does the attribution for you.
  It ships in the nano-ros source tree, not in a release; running it: see
  [Internals → The memory report](../../internals/realtime/mem-report.html).

On the Tutorial's board, `mps2-an385-baremetal`, two figures are worth
knowing before you read either output:

- **The heap is a static.** `nros_platform_mps2_an385::memory::HEAP` is a
  128 KiB allocator (`NROS_HEAP_SIZE` changes it, in bytes), so it is
  already inside the RAM total. Its symbol reads a little over 128 KiB
  (132,816 B measured) because the allocator's own state sits beside the
  arena. It starts all-zero, so it is in `.bss` and costs no flash.
- **The stack has no budget.** The main stack starts at the top of the
  board's 4 MiB SRAM and grows down toward the end of `.bss`, with no guard
  between them; nothing in the image reserves or checks a stack size. On this
  board that leaves about 3.8 MiB, so it is not the limit; on a board with
  less RAM, the stack is what is left after `.data` and `.bss`.

You get RAM broken down by symbol, by owner, by storage role and by pool, plus
the section totals, so you can see how much is *not* attributable to any
symbol:

```
RAM (writable allocated sections): 518,234 bytes
RAM attributed to symbols:         489,771 bytes
unattributed (padding, linker reservations, symbol-less data): 28,463 bytes (5.5%)
```

That last line matters. Alignment padding and linker-script reservations are
real RAM that no symbol names, so a budget built only from a list of pools will
come up short. "Writable allocated sections" means every section the linker
marks allocated *and* writable, whatever it is called — on Zephyr that includes
the per-file `.noinit.*` sections holding thread stacks and the kernel heap.

## Who owns the bytes

The `RAM by owner` table puts every byte under the owner you would name:

```
       177,112   34.2%  [executor storage]
       206,358   39.8%  nros_rmw_zenoh
        88,280   17.0%  (C / asm / no path)
         8,064    1.6%  [component storage]
```

* `[executor storage]` — the executor's backing, whichever language placed it:
  the C/C++ entry's `__nros_executor_storage` / `__nros_sched_task_executor_storage`,
  the C++ boot storage `Node::GlobalStorageHolder<0>::storage`, and the Rust
  `EXECUTOR_BACKING` / `__NROS_SCHED_TASK_EXECUTOR_BACKING`. In a sched-task image this
  is usually the largest single owner.
* `[component storage]` — the generated per-component storage
  (`__nros_comp_buf_N` for C++, `__NROS_COMPONENT_<pkg>_SLOT_STORE` for Rust).
* A Rust crate name, `C++ <namespace>`, or `(C / asm / no path)` otherwise. The
  language comes from the symbol's mangling, so a C++ namespace that happens to
  share a Rust crate's name is never filed under that crate.

For the C and C++ executor storage, the report also reads the build's own sizes
header and tells you how many executors' worth the image reserved:

```
        95,904  executor storage   __nros_sched_task_executor_storage
                                   = 4 x 23,976 (NROS_CPP_EXECUTOR_STORAGE_SIZE = 23,976)
```

The header is found beside the image (the build's `nros-cpp-generated/` mirror);
pass `--sizes-header <path>` if your build keeps it elsewhere. If the header is
newer than the image, or two headers disagree, the report says so instead of
pricing.

## Finding what to cut

The per-symbol list is sorted, so the first few lines are usually the whole
story:

```
## top 5 RAM symbols

       131,072   25.3%  nros_rmw_zenoh::shim::subscriber::LARGE_PAYLOADS
        88,560   17.1%  linux_entry::__nros_entry_run::__NROS_SCHED_TASK_EXECUTOR_BACKING
        88,552   17.1%  nros_node::executor::backing::EXECUTOR_BACKING
        87,072   16.8%  g_sessions
        33,536    6.5%  nros_rmw_zenoh::shim::service::USER_SERVICE_INBOX
```

The last two sections of the report join these symbols to their knobs for you:
`declared pools` lists the pools that carry a formula (and whether the image
agrees with it), and `knob-sized pools` lists every other static the inventory
knows a knob for, with its exact measured size:

```
        87,072  g_sessions
                pool sized by NROS_ZENOH_MAX_SESSIONS, NROS_ZENOH_MAX_PUBLISHERS, ...
```

Be aware that today these pools are sized by which backend you link, **not** by
what your node actually does: a publisher-only node still reserves the service
and large-payload pools in full. If your image looks far larger than the entities
you created would suggest, that is expected rather than a misconfiguration on
your side. Turning the corresponding knobs down is the current remedy (see
[Which knobs the build derives](size-an-image-from-its-contract.md#which-knobs-the-build-derives)).

The zenoh service inboxes (`USER_SERVICE_INBOX`, `ACTION_INBOX`,
`BUILTIN_INBOX`) are one table per family, each sized for its own requests;
how their slots are sized is in
[Static memory and pools](../../background/static-memory-and-pools.md#the-three-service-inboxes-zenoh).

## Showing a saving

Take a baseline (the report's `--json` output), change something, rebuild,
and compare against it (`--baseline`; the commands are on the
[Internals page](../../internals/realtime/mem-report.html#showing-a-saving)).

Every symbol row then carries an annotation: `(+12,288)` for a change, `(=)` for
a symbol that matched and did not move, `(new)` for one the baseline does not
have. A `baseline join` section lists what matched, what is new and what is
gone, and the owner table shows each owner's delta — so a pool that disappears
entirely is reported, not silently absent.

Symbols are matched across builds by name with the compiler's per-build
decorations removed (`.llvm.<hash>`, `.0`, `.constprop.0`, Rust crate hashes),
so a static the linker renamed between two builds still compares. A baseline
written by an older version of the tool lists only its top symbols; the report
marks it incomplete and will not call a symbol "new" on its evidence.

This is also how a change to nano-ros itself should report a memory saving: as
a measured difference between two named images, not as an estimate.

## Cross-compiled images

The tool prefers `llvm-nm`, which reads ELF files for any target; your host's
GNU `nm` is built for one target family and refuses a cross-built image with
*"File format not recognized"*.

You probably already have `llvm-nm` without knowing it: rustup ships it as part
of the `llvm-tools` component, under the toolchain's own `bin` directory rather
than on your `PATH`, and the tool looks there. If it reports that it found no
usable `nm`, run:

```sh
rustup component add llvm-tools
```

`llvm-cxxfilt` (from a system LLVM) is used when present to demangle; without
it the tool falls back to `nm -C`.

## See also

- [Size an image from its contract](size-an-image-from-its-contract.md): the
  pools the build sizes for you, and how to override one.
- [Static pool inventory](../../reference/static-pool-inventory.md): every
  sizing knob and its default.
- [Static memory and pools](../../background/static-memory-and-pools.md): why
  nano-ros reserves its memory statically.
