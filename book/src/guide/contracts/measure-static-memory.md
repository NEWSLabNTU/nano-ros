# Measuring Static Memory

<!-- type: how-to · level: 3 -->

The [Static Pool Inventory](../../reference/static-pool-inventory.md) tells you
which knobs exist and what each one costs: a byte figure at its default where
the pool is a plain product of knobs, and otherwise *where* its bytes go — a
named static, the executor's storage, a heap — and why no figure can be stated
without building. It cannot tell you what *your* image costs — your knobs
differ, your backend differs, and several pools are sized by a `sizeof` that no
comment can see.

For that, measure the image you built with the memory report, which reads
the linked ELF. The report ships in the nano-ros source tree, not in a
release; running it: see
[Internals → The memory report](../../internals/realtime/mem-report.html).

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
  `EXECUTOR_BACKING` / `__NROS_SCHED_TASK_EXECUTOR_BACKING`. In a tiered image this
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
[Sizing the pools](#sizing-the-pools) for which knobs the build already
derives).

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

## Sizing the pools

Before you turn a knob down, know which pools you size and which the build
sizes for you. The pools that dominate static RAM are **derived per image**
from what the image's components declare, and the build writes the answers
into its generated cargo `[env]` table (on Zephyr, into the Kconfig defaults).
Stating one of them by hand is an override, not a requirement.

| Knob | Derived from |
| --- | --- |
| `NROS_ZENOH_MAX_PUBLISHERS`, `NROS_ZENOH_MAX_SUBSCRIBERS` | the declared entity inventory, floored at 1: each sizes a fixed C array, where zero is not a smaller pool |
| `NROS_ZENOH_MAX_QUERYABLES` | the declared service servers, plus the parameter and lifecycle services the image's nodes register; with no readable declaration it stays at its undeclared budget (8 embedded, 32 hosted) |
| `NROS_ZENOH_MAX_LIVELINESS` | one per node name plus the session's own, one per publisher, subscriber, service server and client, and the parameter and lifecycle families' entities |
| `NROS_EXECUTOR_MAX_CBS`, `NROS_EXECUTOR_ACTION_CLIENTS`, `NROS_RMW_SUBSCRIBER_SLOTS` | the declared entity inventory |
| `NROS_SUBSCRIBER_BUFFER_SIZE` | the largest *small*-class message type the image subscribes to |
| `NROS_ZENOH_SUBSCRIBER_LARGE_SIZE` | the largest *large*-class type; stated only when the large count is non-zero |
| `NROS_ZENOH_MAX_LARGE_SUBSCRIBERS` | how many subscribed types fall in the large class. Zero is an answer, not a floor: the pool becomes zero bytes |
| `NROS_SUBSCRIPTION_BUFFER_SIZE` | the executor's take buffer, from the same message bounds |
| `NROS_EXECUTOR_ARENA_SIZE` | the same inventory, in the executor's own build; the variable is the override |

The transport's wire buffers (`NROS_ZENOH_FRAG_MAX_SIZE`, the batch sizes,
`NROS_ZENOH_SERVICE_BUFFER_SIZE`, `NROS_ZENOH_GET_REPLY_BUF_SIZE`,
`NROS_ZENOH_MAX_PENDING_GETS`), the smoltcp socket pools and the parameter
store limits are not derived: they are yours. They are covered in
[Tune throughput and latency](../rmw/tune-throughput-and-latency.md#set-the-wire-buffers)
and listed in the [Environment variables reference](../../reference/environment-variables.md#buffer-tuning).

**Precedence.** A value you state wins over a derived one: an exported
environment variable first, then the image's `[image.<id>.knobs]`, then the
board descriptor's `[knobs]`. The derived value comes next, and the crate's
own default applies only when nothing is stated and nothing can be derived.
`nros config explain` shows which rung answered for each knob.

The derived message sizes are upper bounds, and one unbounded type stops the
derivation; the build names the type in both cases. Why, and the rest of the
model, is in
[Static memory and pools](../../background/static-memory-and-pools.md#where-a-pools-size-comes-from).

Every slot of a fixed pool costs its bytes whether or not an entity uses it,
so an over-provisioned count is wasted RAM and an under-provisioned one is a
create that fails at run time. The [Static Pool Inventory](../../reference/static-pool-inventory.md)
gives each pool's cost at its defaults; the memory report above gives what
your image actually reserved.

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
