---
id: 1442
title: "The launch-declared identity reaches `run_components` and not `run_tiers`
  — four C-ABI tier runners pass a NULL namespace, and the hosted Rust one
  passes no identity at all"
status: resolved
type: bug
area: boot, boards
related: [rfc-0045, 0794, 1050, 1410, 1434, 1443, 1444]
resolved_in: 2026-09-22
---

## Problem

Issue 1434 threaded the `.nros_boot_config` namespace from the generated entry
down to `nros_cpp_init`, which has always had a parameter for it. It did that
on the SINGLE-EXECUTOR path only. The tiered path is untouched, and it is
untouched in two different ways.

**The four C-ABI tier runners still pass NULL.** Measured, all four:

```
packages/api/nros-cpp/src/lib.rs           nros_board_native_run_tiers
packages/boards/nros-board-freertos/c/freertos_run_tiers.c:561
packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c:598
packages/boards/nros-board-nuttx-qemu/c/nuttx_run_tiers.c:685
```

each reaching `nros_cpp_init(locator, domain_id, sn, NULL, boot_storage)`. The
generated entry's `run_tiers` call carries `nros_boot_config_node_name(...)` and
nothing else; both boot wrappers
(`packages/cli/nros-cli-core/src/codegen/entry/packs/entry/{c,cpp}/boot_wrapper.jinja`)
name this issue at the call site.

**The hosted Rust `run_tiers` carries no identity at all.**
`nros-board-linux::run_tiers` ignores its `DeployOverlay` by a decision that
predates this (issue #48, "kept for signature parity"), so before 1434 it opened
its session with `ExecutorConfig::from_env()` — the env rung over nothing. 1434
gave it the baked NAMESPACE and deliberately left `node_name` alone, because
threading that changes what `ros2 node list` prints for every tiered native
image and is a decision on its own.

## Why it was carved out of 1434

Cost and reach, both concrete:

* Covering the C-ABI half means `_ns` twins of four more symbols, on top of the
  two 1434 added — and each of the three RTOS tier runners is its own file, so
  that is four more places for the copies to drift, which is exactly what
  `nros_rtos_run_components.c` exists to avoid on the components path.
* The blob only ever carries IDENTITY for a one-node plan
  (`boot_config_view`: `if plan.nodes.len() != 1 { return }`), and a tiered plan
  rarely is one. So the reachable population is a single-node image that also
  declares tiers.
* A tiered image's PER-NODE namespace already travels, by a different road: the
  `(node name, node namespace, group)` triples issue 1172 added to
  `nros_native_tier_spec_t`. What is missing is the SESSION rung — the identity
  the executor opens with, and the field a post-link patcher rewrites.

## Acceptance

A single-node tiered image — one C or C++, one Rust — whose launch declares a
namespace opens its primary session under it, asserted on the ON-WIRE name. Plus
the negative direction 1434 already pins one layer over: a blob with
`BOOT_SET_NAMESPACE` clear must leave the next rung speaking, never deliver `""`.

Decide `node_name` on `nros-board-linux::run_tiers` in the same change, or say
why it stays: leaving half the identity threaded is the shape that made this
issue necessary.

## Resolution

Resolved 2026-09-22. All five sites read exactly as filed — re-measured before
anything changed, not taken from 1434's write-up:

```
packages/api/nros-cpp/src/lib.rs:4600            nros_cpp_init(NULL, 0, name, core::ptr::null(), sptr)
packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c:598      nros_cpp_init(locator, domain_id, sn, NULL, boot_storage)
packages/boards/nros-board-nuttx-qemu/c/nuttx_run_tiers.c:685   nros_cpp_init(locator, domain_id, sn, NULL, boot_storage)
packages/boards/nros-board-freertos/c/freertos_run_tiers.c:561  nros_cpp_init(locator, domain_id, sn, NULL, boot_storage)
packages/boards/nros-board-linux/src/lib.rs                     resolve_hosted(BootConfig { namespace: blob.namespace, ..Default })
```

The hosted Rust runner's `node_name` was absent exactly as filed, and the
carve-out comment naming this issue was sitting on the line.

### The mechanism is 1434's, extended — not a second one

* **`_ns` twins of the four C-ABI tier runners**, additive, with the old
  spellings forwarding `NULL`: `nros_board_native_run_tiers_ns` (nros-cpp) and
  `nros_board_{freertos,zephyr,nuttx}_run_tiers_ns` (each board crate's
  `c/<rtos>_run_tiers.c`). The reason is `nros_cpp_init_rmw` over
  `nros_cpp_init` (issue 1050) one layer up and unchanged: an entry TU
  generated before this still calls the older name, and an entry TU outlives
  the library it was generated against. Measured on the built object — both
  spellings are defined:

  ```
  $ nm -g --defined-only target/debug/deps/libnros_cpp-*.rlib | grep run_tiers
  T nros_board_native_run_tiers
  T nros_board_native_run_tiers_ns
  ```

* **C++ is header-template delegation**, so no ABI moves and the call NAME does
  not change: every board class in `<nros/main.hpp>` gains
  `run_tiers(session_name, node_namespace, tiers, n_tiers)` and the old arity
  delegates with `nullptr`. `LinuxBoard` needed no new `friend` line (that was
  1434's `nros::init` overload, not a board method).

* **`BoardFamily::c_abi_runners` names the `_ns` spellings**, which is what
  moved the goldens: 8 files, 11 lines (one call line each, plus the banner on
  the three C tier goldens — the banner interpolates the runner name, so it
  followed for free). `c_abi_runners_name_only_symbols_that_exist` passes, with
  its `nros_board_threadx_run_tiers` negative control still absent in BOTH
  spellings.

* **ThreadX is untouched and correct**: it has `run_components` and no
  `run_tiers` (issue 1286), so a tiered ThreadX plan takes the sched-context
  path in both packs. That is the `Option` in `CAbiRunners` doing its job.

One thing 1434's shape did NOT cover and this needed: **cbindgen emitted the new
symbol into `nros_cpp_ffi.h` naming `NativeTierSpecC`, a type that same
exclusion list removes** — so the generated header stopped being
self-contained. `nros_board_native_run_tiers` was already excluded for exactly
this reason; the `_ns` twin joins it, and the comment now says the rule rather
than the instance.

### The two halves, kept apart

They are one seam and two defects, so they are attributable line by line:

* **1442's own work (the NAMESPACE)** is the four C-ABI runners, their C++
  overloads, the two boot-wrapper templates, the `c_abi_runners` ledger and the
  goldens. Tests: `the_baked_rung_carries_the_blobs_namespace` and
  `an_unbaked_namespace_stays_absent_in_the_baked_rung` in `nros-board-linux`.
* **The older `node_name` gap (issue #48, "kept for signature parity")** is one
  field in `hosted_baked_rung`. Test:
  `the_baked_rung_carries_the_overlays_node_name`, which also pins that the
  value comes from `DeployOverlay::node_name` and NOT from the blob — the
  fixture bakes `frombake` into the blob precisely so the two can be told apart.

Both funnels of `nros-board-linux` (`boot_hosted`, `run_tiers`) now build that
value in ONE place, `hosted_baked_rung`. They had been building it separately,
which is the entire reason `node_name` reached one and not the other; a shared
helper is the structural half of the fix, and it is what makes the four unit
tests bind both funnels rather than one. Mutation-checked: reverting the two
fields to `None` fails exactly the two positive assertions and leaves the
negative ones green.

`golden::branch_of` needed the same class fix 1434 applied to its
`run_components` half — its `run_tiers` probe was still an exact
`contains("run_tiers(")` and would have reported "entry took no recognisable
branch" for all four new spellings. Both branches now go through ONE
`calls_runner(src, bare)` helper rather than a second copy of the predicate.

### The precedence rule, and it is 1434's

* **Hosted** (`nros_board_native_run_tiers_ns`, `LinuxBoard::run_tiers`):
  `$NROS_NODE_NAME` / `$NROS_NODE_NAMESPACE` > bake > compiled default. Free,
  because `try_resolve_with` reads `env.node_name.or(baked.node_name)` and
  `env.namespace.or(baked.namespace)` — filling the baked rung costs the
  environment nothing; before, there was simply nothing for it to outrank.
* **Embedded** (the three RTOS `_ns` runners): the bake IS the answer, there
  being no environment rung; the locator and the domain stay the board's, baked
  by cmake through `<nros/entry_config.h>`.
* **Connect facts stay env-driven on the host** (issue #48). Identity is not a
  connect fact — that is the distinction 1434 drew, and the tiered path needs no
  different one. `the_baked_rung_names_no_connect_facts` pins it, because the day
  a locator appears in that rung `$NROS_LOCATOR` stops being authoritative for
  every hosted image.
* **NULL and empty are one case**, resolving to the ROOT and never to `""`, at
  three independent edges (the C++ header, the Rust runner, the C runners). In
  nros-cpp that test is now ONE function, `optional_cstr_arg`, shared by the two
  hosted runners that take the argument; 1434 wrote it inline in the first.

### What was OBSERVED, and what was not

**Observed on the wire — the hosted Rust tiered road.** A probe driven through
`LinuxBoard::run_tiers` with a real `.nros_boot_config` and one tier, against
ROS 2 humble's `rmw_zenohd` (paired `libzenohc.so`, multicast off, a private
port and domain), a generic publisher on the node so the node liveliness token
is declared — `ensure_node_liveliness` runs at ENTITY create, never at node
build, which is why an earlier run of the same probe showed nothing:

| bake | env | `ros2 node list --no-daemon` |
| --- | --- | --- |
| `/island` | — | `/island/probe` (+ `/probe`) |
| none | — | `/probe` |
| `/island` | `NROS_NODE_NAMESPACE=/fromenv` | `/fromenv/probe` (+ `/probe`) |

Positive, negative and precedence rungs, all three measured. This is the same
table 1434 measured on the single-executor road, now measured on the tiered one.

**Observed on the wire — the `node_name` half, before and after.** Same harness,
`deploy.node_name` varied:

| `DeployOverlay::node_name` | before the fix | after |
| --- | --- | --- |
| `Some("tiered_talker")` | `/node` | `/tiered_talker` |
| `None` | `/node` | `/node` |

The before column is the tree with the two fields reverted to `None` and the
probe rebuilt — not an inference.

**NOT observed: the C and C++ roads were never put on a bus.** The C-ABI
runner's new symbol is defined (`nm`, above), the headers compile and link
(`just check c`, `just check cpp`, both green, including the cross-include TU),
and the NuttX and Zephyr tier runners compile clean under host gcc `-std=c11
-Wall -Wextra -Wmissing-prototypes`. The FreeRTOS one was NOT compiled here — no
FreeRTOS headers are provisioned in this worktree — so its edit rests on being
textually the same three-part change as its two siblings, and on the FreeRTOS
lane. A running tiered C image needs the cmake/west lane (it wants a strong
`nros_app_register_backends` and the zenoh-pico C library, which an
`extern`-only probe cannot supply), and that was out of budget here.

And the finding 1434 recorded still holds one road over, unchanged by this fix:
on the C and C++ roads the namespace is delivered correctly into
`ExecutorConfig` — which is what a post-link patcher and every rung above need —
and there is currently no wire-visible consumer of it there, because
`nros_cpp_node_create` writes `"/"` for every node (issue 1443) and the CFFI
session open drops `RmwConfig::namespace` (issue 1444). The `(+ /probe)` column
in the table above is that same session token, at the root, on the Rust road
too.

### Verification

`cargo +nightly fmt` (both workspaces) + `clang-format` on `main.hpp` (17.0.5;
HEAD's copy is byte-identical under it, so no spurious reflow). `just check c`,
`just check cpp`, `just check fast`, `just check api-parity`, `just check
cli-fresh`, `just check platform-abi-mirror`, `just check board-abi-mirror`,
`just check abi-bindings`, `just check ffi-struct-mirrors`, `just check
cli-source-dirs`. `cargo test -p nros-cli-core --lib` 1343 passed, `--test
board_key_table` 8, `-p nros-entry-lower`, `-p nros-board-linux` 6.
`nros_cpp_ffi.h` regenerated with `just regen-c-headers` (net zero change, once
the new symbol was excluded). The unsafe census grows by 3/1/1 in nros-cpp — the
`optional_cstr_arg` helper plus the tiered runner's two new `unsafe` blocks —
and the baseline is rewritten to say so.

Sweep command for the class, for whoever adds the next runner:

```
grep -rn 'nros_cpp_init(' packages --include='*.c' --include='*.rs' | grep -v generated
```
