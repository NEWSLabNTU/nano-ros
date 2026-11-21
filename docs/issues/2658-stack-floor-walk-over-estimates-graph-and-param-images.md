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
