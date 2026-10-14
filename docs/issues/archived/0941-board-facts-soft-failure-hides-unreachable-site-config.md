---
id: 941
title: "`nros_resolve_board_facts` fails SOFT, so an unreachable site-config block is silent"
status: resolved
resolved: 2026-10-14
area: build
severity: medium
found: 2026-08-31
related: [0940, 0949, 0950, phase-405, RFC-0072, phase-484]
---

**Home: [phase-484](../../roadmap/archived/phase-484-one-owner-per-fact.md)** — re-homed 2026-10-12 from phase-405, which is archived.

# A resolution that cannot succeed prints STATUS and continues

`nros_resolve_board_facts` treats an unresolvable `[deploy.<t>.nros]` lookup as
SOFT: the configure prints a STATUS line and carries on, so the image builds
without `NROS_BOARD` / `NROS_BOARD_TOML` / `NROS_NETSTACK` rather than failing.

That is why issue 0940 was invisible. Not one block — **twelve**, across
**seven** bringups in six workspaces, every one of them declaring a netstack and
a pair of SDK roots that reached no build:

```
c/demo_bringup            [deploy.freertos]  [deploy.nuttx]
cpp/demo_bringup          [deploy.freertos]
mixed/demo_bringup        [deploy.freertos]
realtime-c/demo_bringup   [deploy.freertos]  [deploy.nuttx]
realtime-c/smp_bringup    [deploy.freertos]  [deploy.nuttx]
realtime-cpp/demo_bringup [deploy.freertos]  [deploy.nuttx]
rust/demo_bringup         [deploy.freertos]  [deploy.nuttx]
```

They survived because nothing said anything. Ten of the twelve happened to have
a byte-identical live sibling in the same file, so nothing broke either — the
configuration was redundant as well as dead, which is the only reason this cost
nothing so far.

## Why 0940 does not close this

Phase-405 W6 removes the twelve instances and stops the generator emitting a
block for a target that names no `board`. That is the CLASS fix for *those*
blocks. It does not change the fact that a configure which cannot resolve board
facts still proceeds quietly — the next way to produce an unreachable block is
still silent.

## The judgement this needs

Not every soft failure here is wrong. A native configure legitimately resolves
no board facts, and hardening this without separating those cases turns every
host build into an error. So the work is:

* enumerate which configures legitimately reach `nros_resolve_board_facts` with
  nothing to resolve;
* make the remainder loud — a `FATAL_ERROR` naming the deploy target and which
  of the two lookup paths was attempted (`board_facts.rs:112` vs `:274`);
* keep the STATUS line for the legitimate set, saying so explicitly rather than
  by omission.

Split from 0940 deliberately: 0940's fix is mechanical and lands now, this one
needs the enumeration first and should not hold it up.

## RE-OPENED (2026-08-31) — the fix was dropped, the defect is not

phase-405 landed reason codes (`mod reason`, eleven machine-readable
`board-facts[<code>]` prefixes) plus a cmake arm that made two of them FATAL.
That work was DROPPED rather than rebased when #0951 redesigned the area: the
codes named a `[deploy.<t>.nros]` shape that no longer exists, and reapplying
them would have described the file's old shape convincingly and wrongly.

**What #0951 fixed here.** `[deploy.*]` is gone, so `unreachable-site-config`
and `deploy-names-no-board` have nothing left to classify. And issue 0949
(fixed separately) removed the single largest cause of silent non-delivery —
a migrated workspace resolving to a directory with no `system.toml`.

**What remains.** `nros_resolve_board_facts` still answers several failures with
`message(STATUS "… NOT delivered …")` and `return()` — no CLI, no workspace dir,
and the general arm. Those are still soft: the configure continues and the image
builds without `NROS_BOARD` / `NROS_BOARD_TOML` / `NROS_NETSTACK`. Main added
`FATAL_ERROR` to some arms independently; the class is narrower than it was and
is not gone.

**The judgement this still needs**, unchanged from the original filing: which
configures legitimately resolve nothing (a native build does), and which should
be loud. That enumeration — 414 probes across 16 workspaces — is recorded above
and is still the right method; the answer just has to be re-derived against
`[board_config.*]`.

## The soft failure now has a price tag: the NuttX nightly (2026-09-13)

Nightly run **34744568635** (schedule, 07:10), job **103690230027** (`nuttx`),
step `Test / e2e (nuttx)`. The configure prints the soft line twice for each of
the **six** NuttX C++ example leaves — `listener`, `talker`, `service-client`,
`service-server`, `action-client`, `action-server` — twelve lines in all:

```
-- nano-ros: board facts NOT delivered from examples/qemu-armv7a-nuttx/cpp/listener
   — Error: examples/qemu-armv7a-nuttx/cpp/listener/system.toml: no [image.nuttx]
   or [deploy.nuttx] naming a board
   Location: nros-cli-core/src/cmd/board_facts.rs:387:24
```

and the job then reports `Real failures: 5 / 5`:

```
FAIL test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_1_Lang__Rust
FAIL test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C
FAIL test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp
FAIL test_rtos_pubsub_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C
FAIL test_rtos_service_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp
```

**What this adds, stated no wider than it is measured.** The `Cpp` cell of
`test_rtos_service_e2e` is a leaf that carries the soft line, so the two are
consistent; the three `action` cells match the shape issue 1341 already records
on FreeRTOS across all three languages, so they are more likely that; and
`test_rtos_pubsub_e2e … Lang__C` is a **C** leaf, which prints no soft line at
all and is therefore NOT explained by this issue. No C leaf warns — the twelve
lines are C++ only.

The point is not the cell count. It is that this issue's own argument —
"the configure continues and the image builds without `NROS_BOARD`" — has been
invisible because nothing downstream said so. A lane that runs those images now
does, and the reach is wider than the original filing: this is the **standalone
example** front door (`examples/qemu-armv7a-nuttx/cpp/*`), not the workspace
bringups the 414-probe enumeration covered. Whatever the judgement call turns
out to be, it has to answer for a `system.toml` with no `[image.<t>]` naming a
board, not only for an unreachable `[deploy.<t>.nros]`.

## Resolution (2026-10-14)

The enumeration the issue asked for, done against main on 2026-10-14:
every exit of `nros ws board-facts` (`cmd/board_facts.rs`) crossed with every
configure shape that reaches `nros_resolve_board_facts`. Two configure shapes
LEGITIMATELY have no board facts, and both are host builds that name no board:

- a guest project, i.e. `find_package(nano_ros)` with no `system.toml` and
  no `Cargo.toml`, or a cargo dir that depends on no board crate;
- a bringup that declares no `[image.*]` at all (only test fixtures do: 7
  tracked `system.toml`, all under `packages/{cli,testing}/…/fixtures`).

Every other failure happens AFTER `nano_ros_read_leaf_system` has validated
the leaf, so it is a real error. Since phase-484 W2b a skipped cross image
also died later in `zpico-sys` on a missing `FREERTOS_PORT`, which is the
quiet half-build this issue is about.

**The verb decides, not a regex.** `nothing_to_carry` answers those two
shapes with exit 0, no stdout, and `board-facts: nothing to carry -- <reason>`
on stderr. Asking for a board or deploy is never "nothing". The cmake arm
(`NanoRosBoardFacts.cmake`) now:
- stops the configure on ANY nonzero exit, naming the workspace, the
  `--board` / `--deploy` it asked with, and the verb's stderr;
- prints the stated reason on STATUS for an empty answer;
- treats a missing CLI as fatal when a `system.toml` is in scope or a board
  is named, and as STATUS otherwise.

The dead `_ws STREQUAL ""` arm is gone (the candidate list ends at
`CMAKE_SOURCE_DIR`, which is never empty).

**Why this does not repeat the two earlier misses.** `21be9c0684` (#606) and
`5d9abbd376` (#606/#590) each made a cmake regex over the verb's error TEXT
fatal, and each broke a lane: an unresolvable board spelling, then
`[deploy.native]` with no `board`. Both inputs are gone (W4a's
case-insensitive aliases and `nano_ros_read_leaf_system`'s earlier refusal;
`[deploy.*]` is refused by the schema). More to the point, "nothing to
carry" is now a typed outcome the verb owns, so a new error message cannot
fall into the skip bucket by accident.

**Measured.** `just build-test-fixtures lane=tier2` on a tree carrying this
change. All 7 families built:
`zephyr linux qemu freertos nuttx threadx_linux threadx_riscv64`.
- 38 configures received board facts.
- The "nothing to carry" lines were the three Zephyr guest apps
  (`board_import_fvp`, `multi_pkg_workspace_zephyr/zephyr_app`,
  `zephyr_self_pkg/sibling/caller`), as enumerated.
- No configure was refused.

Test: `only_a_configure_that_names_no_board_has_nothing_to_carry` covers the
guest dir, a cargo dir on no board crate, a bringup with no image, a named
board, and an image naming an unknown board.

Found on the way and fixed separately: the host census configured as the
retired `native` board (#2182), and a cmake function built a path from
`CMAKE_CURRENT_LIST_DIR` (#2186). Each stopped the tier-2 fixture build
before this change could be measured.

