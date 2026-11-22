---
id: 2658
title: "The stack-floor walk refuses an image that queries the graph or declares a parameter, asking 3-10x the stack it uses"
status: open
type: limitation
area: build, stack-floor, zephyr
severity: medium
found: 2026-11-16
related: [issue-2360, issue-2202, issue-2402, phase-557]
---

**Home: [phase-557](../roadmap/phase-557-stack-floor-measured-frames.md).** The
walk binds an indirect call to every function its type allows, so a path it
cannot follow becomes a chain of every candidate. This
issue is measured evidence for the stack-floor redesign being weighed, from two
images that do things the templates do not.

## What was measured

Two Nucleo-F767ZI images, made with `nros new` from the `talker` template the
way `just zephyr stack-floor-twin` makes them, then changed:

- **C talker:** calls `rclc_executor_get_topic_names_and_types` and
  `rclc_executor_get_publisher_names_and_types_by_node` on every tick, with
  graph discovery switched on in the image
  (`[image.zephyr.knobs.zenoh] graph_discovery = true`,
  `graph_cache_size = 8192`). Both calls return `RCL_RET_OK`.
- **Rust talker:** declares an integer and a string parameter in `register()`
  (`param-store`, `[image.zephyr.knobs.params] max_parameters = 2`).

Each image was built with the stack-floor lane's `twin.conf` (the thread
analyzer, `CONFIG_INIT_STACKS`) and run in the Renode twin for 75 s: 12 to 14
analyzer rounds and 61 to 66 publishes, with no errors. Renode is
deterministic, so every round agrees.

| image | the walk's floor | measured main peak | floor / peak |
|---|---:|---:|---:|
| C talker, graph queries | 122,120 B | 12,844 B | 9.5x |
| Rust talker, two parameters | 76,816 B | 27,388 B | 2.8x |

For comparison, the unchanged templates' walks on this board are 16,112 B (C)
and 22,212 B (Rust), per phase-557 W6's first run.

At the lane's 64 KiB main stack the build REFUSES both images
(`the OPEN stack is 65536 B, below the measured floor ...`), so a user who adds
one graph query to the C template must give main 122 KiB of the board's
384 KiB to link it, against the 13 KiB the image uses.

## Why the floor is that large

The C image's worst path, from the walk's `--json` output, is 460 frames long.
It runs from `main` through the executor's open, logger dispatch, an action
server's goal iteration, `zephyr_thread_wrapper`, the UART driver's
`uart_stm32_configure` and the graph-change callback. Most links are indirect
calls bound by type. No frame on it is larger than 4,240 B, so the floor is a
long sum of small frames on a path no thread runs, not one large frame.

The Rust image's worst path is 273 frames, the same shape. Its largest frames
are the component install (8,040 B), an action server's registration
(6,056 B) and the parameter declaration (3,720 B), and the image declares no
action server.

## What would close it

A walk whose floor for these two images is within the lane's margin of the
measured peak, or a stated reason why the gap is the price of soundness. The
two images are the test: they are reproducible from the steps above, and the
twin measures them in under two minutes each.

## Two narrowings, measured before building either (2026-11-20)

Two narrowings were proposed that do not need the parked binding work of
[issue 2550](2550-stack-walk-order-dependent-under-cycles.md):

- **(a) thread entries.** A function whose address is handed to a thread
  spawn runs on its own stack, so drop it as an indirect callee and walk it
  as its own root, against that thread's declared stack.
- **(b) device-API slots.** Bind a Zephyr `api->fn` call only to the
  functions stored in initializers of that API struct type, not to every
  function of the same signature.

Each was priced first, as an upper bound: the recorded walk command of
`talker/c/zenoh@nucleo-f767zi-zephyr` (the drift build at `0bb5478c70`)
replayed with `--cut` at the frames each narrowing would remove. A cut keeps
the frame but stops the walk beneath it, so it removes at least as much as
the narrowing would. The row's measured main peak is 12,844 B.

| cuts | walk | path |
|---|---:|---:|
| none | 112,556 B | 448 frames |
| (a): every thread entry and ISR on the path (`zephyr_thread_wrapper`, `z_thread_entry`, `*_task_fn`, `work_queue_main`, the executor and scheduler trampolines, `*_isr*`) | 100,820 B | 388 |
| (a), plus (b) on the driver and fd APIs (`uart_*`, `gpio_*`, `console_*`, `zvfs_*`, `stm32_*`, `clock_control*`, `eventfd*`), plus every callback-signature hop (`*_trampoline`, `*_cb`, `sample_handler`, `queryable_callback`, the `_elem_free`/`_elem_clear`/`_elem_copy` container callbacks) | 53,396 B | 167 |

The last row is generous: it cuts callbacks a real narrowing would keep,
because some of them do run on main. Even so, it leaves the walk 4.2x the
peak. Its worst path is still made of hops bound by signature: logger
dispatch into the action executor, an action server's goal status into
publisher event slots, `z_timer_expiration_handler`, the serial read path,
and session teardown through `rc` deleters and arena drops. No thread runs
that path.

**So neither narrowing is built.** (a) alone moves the floor by about 10 %.
(a) and (b) together, at their most optimistic, cannot bring it within the
lane's margin of the peak. What closes this issue is binding an indirect call
to the targets that can actually reach that site, not to every function of
its type: step 2 of issue 2550 (field- and parameter-sensitive binding,
stopped at its first checkpoint, phase-557 W7f). That work stays parked, and
this issue waits on it.

## The existing classifier, measured against it (2026-11-22)

Phase-557 W7f built the field- and parameter-sensitive classifier this issue
waited on (W7f.0-W7f.2, `--classify-sites`). Its Checkpoint 2 stopped W7f,
because the SCC stays at about 2,000 functions. The question here is
narrower: does binding the sites it DOES resolve move the floor these images
are refused on? Measured with `NROS_STACK_W7F_CHECKPOINT`: the same 8-order
walk, on the graph with every resolved site bound to its channel's set.

| image | walk today | walk, resolved sites bound | measured main peak |
| --- | ---: | ---: | ---: |
| stock `talker/c/zenoh@nucleo-f767zi-zephyr` | 108,800 B | 100,448 B (-7.7 %) | 11,876 B |
| stock `talker/rust/zenoh@nucleo-f767zi-zephyr` | 68,568 B | 65,868 B (-3.9 %) | 28,340 B |

So it does not close this issue. The two images above are those templates
plus a graph query or two parameters, and they are refused for the same
inflation.

**What the remaining path is made of.** The C talker's re-bound deepest path
is 378 frames. Each of its 154 indirect hops is attributed to why its call
site stayed bound:

| hops | frame bytes | why the site stays bound |
| ---: | ---: | --- |
| 99 | 32,608 | a Rust `dyn` call, bound by vtable slot (W7) to every function at that slot in any vtable |
| 16 | 744 | a machine-level site with no IR |
| 31 | about 2,700 | C sites the classifier cannot resolve (`_z_link_t` and `_z_transport_multicast_t` reachable across Rust<->C, loads the rules do not place, casts, phi) |
| 4 | 32 | resolved, still on the path |

The C image's floor is mostly made by the RUST runtime it links (`nros-c`
is Rust): trait-object dispatch, bound by slot to every implementation in
the image. The C classifier does not reach those sites. Closing this issue
needs Rust-side points-to, knowing which concrete vtable reaches each `dyn`
call. That is the class of work issue 2550 parks, so this issue stays
waiting on it, and W7f's measurement tools re-measure it in one run.

## Measured by class, and the first narrowing built (2026-11-23)

Two images: the Nucleo Rust action server row
(`action-server/rust/zenoh@nucleo-f767zi-zephyr`, walk 113,808 B, twin peak
30,328 B) and realtime-c's `[image.freertos]` (mps2-an385; OPEN 188,308 B and
TASK_HIGH 191,436 B, no measured peak). Each image's own walker command was
replayed in-process with a class of bound edges removed after bind, and the
same 8-order walk ran again. A removal is an upper bound on what a narrowing
could buy.

Removing single edges does not work: twelve times removing the first loose
edge of the deepest path moves the Nucleo walk only to 102,536 B, because
every hop has alternatives at about the same depth. Classes:

| removed | Nucleo | realtime-c OPEN |
|---|---:|---:|
| none | 113,808 | 188,308 |
| thread entries, ISRs and device inits as callees | 102,912 | 186,028 |
| `type` joins of untyped GCC C | 106,496 | 168,128 |
| `c_type` edges from C sites | 81,088 | 178,740 |
| `machine` edges | 113,808 | 119,236 |
| every edge looser than a slot (the bind ceiling) | 50,984 | 32,824 |

### What was wrong: evidence from code the link discarded

On realtime-c, 414 of the 425 functions that bind counted as C-reachable
only through dropped code are not in the image at all, so bind was binding
C sites to code that does not exist. Two sources:

- **Relocations of discarded sections.** The walk reads every input object,
  including sections the link drops. The force-link anchors
  (`C_SURFACE_ANCHOR`, `CPP_SURFACE_ANCHOR`: a table of every FFI export,
  kept in the archive and dropped from the image) put every `nros_cpp_*`,
  `rclc_*` and `rcl_*` export in `c_taken`.
- **Rust IR globals the image does not hold.** The same anchors in the
  bitcode put every export in a constant table.

So every machine site in zenoh-pico's GCC-built container helpers
(`_z_svec_*`, `_z_slist_*`, `_z_list_*`) was bound to the whole C/C++ API.

**The rule now:**

- A relocation's address takes are set aside only when BOTH signals say its
  section is gone: none of the names it defines is in the linked image's
  symbol table, AND the link map says the link discarded it (GNU ld's
  "Discarded input sections", or, for LLD, a section of a placed member that
  the map does not place). A stripped or renamed name alone drops nothing.
- A Rust IR global is set aside on the same two signals, by its own data
  section's name.
- A function whose address the image holds counts whatever the inputs say: a
  pointer-sized word at any byte offset of any allocated section, executable
  ones included (literal pools, tables a linker script writes). The first
  oracle run found why that part is needed: Zephyr's `sw_isr_table` is rebuilt
  at a second link stage from the discarded `.intList`, with literal addresses
  and no relocation.

Every unsure case keeps the evidence: no map, no image, an object the map
never names, a section with no symbol a symtab keeps, a name any kept object
also uses, or a `private` IR global.

**Checked against the image.** For every function that lost evidence, the
image's allocated, non-executable sections were scanned for its address.
The executable sections were scanned too. There were 0 hits on both images.

| image | before | after |
|---|---:|---:|
| Nucleo action server | 113,808 B | 103,240 B (-9.3 %) |
| realtime-c OPEN | 188,308 B | 92,324 B (-51 %) |
| realtime-c TASK_HIGH | 191,436 B | 90,300 B (-53 %) |

Every row of the stack-floor table was re-walked by the drift check and
re-recorded. None failed. Every `[[observed]]` row is sized from its peak, so
no stack moves:

| row | was | now |
|---|---:|---:|
| `action-server/rust/zenoh@nucleo-f767zi-zephyr` | 111,136 | 103,240 |
| `talker/rust/zenoh@nucleo-f767zi-zephyr` | 69,648 | 66,776 |
| `action-server/rust/zenoh@mps2-an385-zephyr` | 117,828 | 108,152 |
| `talker/rust/zenoh@mps2-an385-zephyr` | 74,220 | 72,084 |
| `qemu_bsp_talker_bringup@mps2-an385-baremetal` | 157,044 | 153,784 |
| `talker/c/zenoh@nucleo-f767zi-zephyr` | 112,200 | 72,000 |
| `talker/c/zenoh@mps2-an385-freertos` (`[[drift]]`) | 117,768 | 67,004 |

### What binding cannot close

The bind ceiling is above the Nucleo image's peak: 50,984 B, 1.68x the
30,328 B the twin measures. That path is all direct calls through large
frames:

- the entity-create closure `out_of_line` (16,824 B);
- `Executor::declare_parameter_overridden` (8,712 B);
- `Executor::seed_use_sim_time_default` (8,664 B).

realtime-c's slot+direct path carries `Executor::pass` (9,832 B) and
`reconcile_parameter_services` (9,800 B).

Those frames are the frame-size batch's work (`fix-2815-frames`), not this
issue's. What is left here is the thread-entry narrowing: walk a thread entry
as its own root rather than as a callee, from the spawn API's entry argument,
the ISR table and the device init table.
