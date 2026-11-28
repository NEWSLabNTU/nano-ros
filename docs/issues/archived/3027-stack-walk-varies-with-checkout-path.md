---
id: 3027
title: "One tree's stack walk moved by up to 2.6 KB with the checkout path, because a data word that equals a function's address by chance counted as a pointer"
status: resolved
type: bug
severity: medium
area: [memory, tooling]
related: [issue-2550, issue-2658, issue-2521, issue-0616, issue-2400, issue-2653, issue-3028]
found: 2026-11-29
resolved: 2026-11-29
resolved_in: "the change that adds this line"
---

## What happens

The same source tree walks to a different stack depth depending on where it
is checked out. `talker/c/zenoh@mps2-an385-freertos` (the `[[drift]]` row,
recorded `static = 52308`), on the tree of PR #5065 replayed onto main
68814fb553 (tree `fdf17f1dc6`), built by
`just zephyr stack-floor-drift --only talker/c/zenoh@mps2-an385-freertos`
in four worktrees:

| worktree | walk |
| --- | --- |
| `rgate-5065` (after `just ci gate`) | 52,948 B |
| `drift-5065` (setup only) | 50,884 B |
| `dk-5065` (setup only), built twice | 52,536 B, 52,536 B |
| `dk-5065b` (setup only) | 50,384 B |

Plain main 68814fb553 walked 52,308 B at two paths. It happens to be stable
there; the class does not depend on it.

The spread is 2,564 B (4.9 %) on one tree. The drift check tolerates a 2 %
rise (`drift_rise_pct`), so whether a PR passes it depends partly on the
directory the merge queue or the gate host builds in. PR #5065's first queue
walk read 56,044 B (+7.1 %); some of that may have been this.

## What it is not

- **Not the walk itself.** Re-running the build's own `nros-stack-floor.py`
  command on the same build gives the same answer every time (52,536 B twice
  on `dk-5065`).
- **Not the environment.** `CARGO_PROFILE_DEV_DEBUG` was unset in every run.
  A setup-only worktree moves as much as one that ran `just ci gate` first.
- **Not shared state.** Every input the walk reads (`--bitcode-dir`,
  `--cc-fragments`, `--compile-commands`, `--llc-cache`, `--su`,
  `--link-map`) resolves inside that build's own
  `build/stack-floor-drift/...` directory. Nothing comes from a shared
  `CARGO_TARGET_DIR` or sccache.
- **Not a frame size.** The call graphs of `dk-5065` and `dk-5065b`, with
  names canonicalised by the walk's own `_order_key` normaliser
  (`_canon_v0` + `_mask_names`), hold 4,971 functions each and have **0
  frame-size differences**. Neither codegen-unit partitioning nor inlining
  changed a real frame.

## Cause

The path reaches the graph's names through cargo's `-C metadata`, which
includes the path spelling (issue 0616's note). That sets the crate hashes,
and the hashes decide which crate's copy of a shared generic the link keeps:

- In `dk-5065` the surviving copies are `nros_c`'s:
  - `arena::guard_has_data` / `guard_try_process` for the closure in
    `rclc_node_create_guard_condition`;
  - `c::service::atomic_bool_waker`'s VTABLE `call_once`.
- In `dk-5065b` they are `nros_cpp`'s:
  - the same two for `nros_cpp_guard_condition_create`;
  - `cpp::service::atomic_bool_waker`.

Each pair is the same code under a different defining crate. `_canon_v0`
drops the INSTANTIATING crate, but these differ in the closure's own path,
so they stay two names.

The path also reaches the graph a second way, through layout. The image
embeds the absolute checkout path in panic-location and file strings: 62
occurrences in `dk-5065` and 63 in `dk-5065b`, with `.text` at 527,004 B and
527,276 B. `image_data_pointers` (issue 2658) counts every data word that
equals a function's address as an address-taken function. When the layout
shifts, the coincidental matches shift with it: 376 such functions in one
build, 371 in the other. That is how plain C functions come and go as
indirect targets between the builds (`_fstat`, `_getpid`, `done_socket`,
`_z_query_clear`, `freertos_assert_failed`).

Both kinds of change land in the address-taken set, so the bind walk's candidate set
for an indirect site changes with them. Between the two builds, **747
functions have a different callee set**. For example,
`nros_cpp_node_get_param_bool` fans out to 605 targets in one and 611 in the
other. On that graph the walk is issue 2550's heuristic: each of its 8 fixed
orders cuts the large cycle differently, and the reported depth is the
deepest order:

```
dk-5065   per order: 47508 44376 49004 48064 46560 51164 44944 52536  -> 52536
dk-5065b  per order: 47500 43900 47808 46792 46696 50384 44564 49888  -> 50384
```

The two worst paths agree for seven frames, from `app_task_entry` down to
`CffiSession::create_subscription`. They part at the type-bound edge into
`drop_entry<SubBufferedRawMsgInfoEntry...>`, then run about 290 frames of
indirect-call chain that no real call stack follows.

## Relation to issue 2550

Issue 2550 is the root: on a cyclic graph the walk is a heuristic, not a
bound, and the max over more orders only rises. This issue is one input that
moves the heuristic's answer for an unchanged program: the checkout path.
Issue 2521 closed the "unlinked copies" input and `_order_key` closed the
"crate hash in the sort key" input. The surviving-copy rename gets past both.

## Fix

The coincidental words are what let the path in, so the fix is at the
evidence, plus the path bytes themselves:

1. **Relocation-backed image pointers** (`image_held_pointers` in
   `scripts/nros-stack-floor.py`). The stack-floor image is linked with
   `--emit-relocs` on all three roads: the cmake FreeRTOS road
   (`target_link_options`), Zephyr (`zephyr_ld_options`) and the cargo road
   (`-Clink-arg`, `nros stack-floor`). A function counts as address-taken
   from the image only where an absolute data relocation wrote its address
   (`ABS_RELOC_TYPES`, per ELF machine; an unknown machine takes every
   relocation). That counts every pointer the linker resolved and no word
   that equals an address by chance. It is not a shrink in the unsafe
   direction: the words it drops were never pointers.
   - A table a link writes without a relocation stays on the word scan, by
     section name (`WORD_SCAN_SECTIONS`: Zephyr's `sw_isr_table`, a linker
     script's `LONG()` vector table).
   - Every word the full scan finds that neither method backs is printed
     and kept in the report (`image_held.unbacked`), so a real table that
     lost its relocation shows up by name. It can then be added to
     `WORD_SCAN_SECTIONS`.
   - An image with no relocation section refuses, naming `--emit-relocs`,
     so a link that forgot the flag cannot drop pointers silently.
2. **Path-independent bytes.** The stack-floor build's own flags rewrite the
   checkout prefix in the image's strings: `--remap-path-prefix` for rustc,
   `-fmacro-prefix-map` for C (`__FILE__` only; the `.su` and DWARF paths
   the walk reads are untouched). They are never global: rustflags are
   fingerprinted, and a global change would re-stale every fixture. The cmake
   roads carry the map; the cargo road (`nros stack-floor`) carries only
   `--emit-relocs`, which is what removes the path's effect on the evidence.

The crate-copy rename needed no change. Only the kept copy is in the graph,
the copies are the same code, and the measured per-order depths do not move
with it.

What stays: any change that shifts the layout still changes which words
equal an address, but those words no longer count. And the walk on a cyclic
graph is still issue 2550's heuristic. This fixes neither; it removes the
checkout path as an input.

## Measured with the fix

The same #5065 tree at three checkout paths (`dk-5065`, `dk-5065b`,
`dk-5065c`) walks **51,204 B at all three**, with identical per-order depths
`47404 44424 49004 46824 46600 51204 44632 49792`. Graphs a and c are
identical; b differs only by the copy rename. Image evidence: 357 functions
relocated, 0 word-scanned, 16, 19 and 21 unbacked words. The union of the
unbacked words is 31 names: float helpers, zenoh-pico session functions,
lwIP igmp, `Clock::now`, two FreeRTOS hooks the kernel calls directly, and
`_Min_Stack_Size`. None is a table entry, and the set changes between
builds, as coincidences do.

## Every row, old evidence against new (2026-11-29)

Each `[[drift]]` / `[[observed]]` row was built once on this change (with
`--emit-relocs` and the path map), then walked by main's
`nros-stack-floor.py` and by this one, with the walk cache off. The two
walks read the same image, so each difference is the evidence change alone:

| row | main's walk | this walk | change |
| --- | ---: | ---: | ---: |
| `qemu_bsp_talker_bringup@mps2-an385-baremetal` | 79,792 | 76,676 | -3,116 |
| `talker/c/zenoh@mps2-an385-freertos` | 52,560 | 52,308 | -252 |
| `action-server/rust/zenoh@mps2-an385-zephyr` | 72,924 | 73,568 | +644 |
| `talker/rust/zenoh@mps2-an385-zephyr` | 71,664 | 71,832 | +168 |
| `pubsub_bringup@zephyr_cortex_m_talker_clock_sntp_resync` (SNTP_RESYNC) | 83,580 | 83,628 | +48 |
| `action-server/rust/zenoh@nucleo-f767zi-zephyr` | 69,448 | 69,448 | 0 |
| `talker/rust/zenoh@nucleo-f767zi-zephyr` | 66,620 | 66,620 | 0 |
| `talker/c/zenoh@nucleo-f767zi-zephyr` | 73,252 | 73,252 | 0 |

`threadx_linux_rs_talker_bringup@threadx-linux` is `unwalked` by design.

- **The bare-metal fall** is one false target. Main's worst path ran through
  the type-bound indirect edge
  `DeclarativeNode::__declare_entity -> _z_interest_send_decl_token.part.0`.
  That function is a GCC `.part` clone that is only ever called directly,
  and its address appears in the image only as a coincidental word: it is in
  this walk's `image_held.unbacked`. The other 26 unbacked words in that image
  are linker symbols (`__exceptions`, `_stack_size`,
  `_nros_stack_guard_size`), `main` (which reset calls directly), and
  functions no table names. `.vector_table` is word-scanned (49 functions).
- **The three rises** cannot be pointers the old scan missed. The new
  address-taken set is a subset of the old one: every relocation-backed word
  is also a word the scan finds, and the named sections are scanned the
  same. Fewer targets letting the walk find a deeper path is issue 2550's
  re-cut. All three are below the drift band.
- **Zephyr's `sw_isr_table`** is word-scanned by name in every Zephyr row
  (11 to 16 functions), as its literal addresses need.

The recorded `static` of both action-server rows is about 33 % above what
main itself walks (103,240 and 108,152 B against 69,448 and 72,924). That
predates this change and is issue 3028. This change re-baselines no row; the
moved rows are listed for the reviewer who owns the table.

## Acceptance

- The same tree, built at three or more checkout paths, walks to the same
  depth: met on `talker/c/zenoh@mps2-an385-freertos`, 51,204 B at all three
  (on the #5065 tree).
- The self-test pins the five behaviours:
  - a relocated pointer is held;
  - a coincidental word is not held, and is reported;
  - a call relocation's site is not a pointer;
  - a named `isr_table` is word-scanned;
  - an image with no relocations refuses.
- Every walkable row walks on all three roads with relocations: the table
  above. Its moves are attributed.
