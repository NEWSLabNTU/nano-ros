# Workspace example layout

`examples/workspaces/{rust,c,cpp}` are read **side by side** — they express the
same system in three languages, so a reader can compare them directly. Their
structure is therefore kept parallel on purpose (phase-331 W2b, RFC-0066).

## Which layout class is this?

The whole-tree taxonomy lives in
[`examples/README.md` § Layout classes](../README.md#layout-classes) — two
questions (*who owns the link?* and *leaf or workspace?*), the classes, and the
two that deliberately do not exist. This file owns the **workspace** half of it.
Survey and work items:
[phase-470](../../docs/roadmap/archived/phase-470-example-layout-unification.md) (done); open gaps:
[phase-477](../../docs/roadmap/phase-477-example-gaps-and-unreported-lanes.md).

Three classes reach this directory, and **the class is a property of an IMAGE,
not of a directory**:

| class | the image's entry | recognise it by |
| --- | --- | --- |
| **1** | **generated** per `[image.*]` (RFC-0098 D9), built by cargo or cmake | `.colcon_workspace` + `src/*_bringup/system.toml`; the image's board is not a Zephyr board |
| **1z** | built through **west** — a Zephyr image. Its west application is **generated** (phase-470 W5) unless a hand-written `src/*_entry` package still claims the row ([RFC-0085](../../docs/design/0085-zephyr-workspace-and-west-handoff.md) D4) | the image's board resolves to a Zephyr board; a still-hand-written one also carries `entry = "<pkg>"` or lands on an existing `<id>_entry` package calling `find_package(Zephyr)` |
| **1b** | none — no bringup at all | `.colcon_workspace` with no `*_bringup`; `nros build` builds every package in dependency order, colcon's default. Both members live in `examples/templates/`, not here |

So `rust/` is not "a class": it declares 17 `[image.*]` rows, 15 of them
class 1 and two — `zephyr` and `zephyr_robot1` — class 1z. Of those two, only
`zephyr_robot1` still has a hand-written entry.

**1z is defined by the ROAD, not by who wrote the entry.** It used to read "a
hand-written Zephyr entry", which made it shrink to zero as phase-470 W5
migrated rows — a class that exists only because something is unfinished, which
is exactly what [`examples/README.md`](../README.md#two-classes-that-deliberately-do-not-exist)
refuses to name ("class 2"). A hand-written entry is a migration STATE of a 1z
image, not a class.

That migration (issue 1288) took 15 hand-written entry packages, serving 16
Zephyr image rows, down to a handful — W5.a for the first Rust image, W5.b1/b2
for the rest of Rust, W5.b3 for six C/C++ images. Each one still hand-written is
blocked by something other than the generator, recorded in issue 1288. **Recount
with the commands below rather than reading a number here.**

Two C/C++ packages are deliberately NOT migrated, and both reasons are about
the generated application's ADDRESS rather than its content. `realtime-c`'s one
package serves an `[image.zephyr]` in two bringups, and the generated directory
is keyed on `coordinate(platform, image)` — (platform, rmw), not the bringup —
so both images resolve to one path and the second write wins (measured,
phase-470 W5.b3). `realtime-cpp/src/fvp_entry` states its own board through
`nano_ros_use_board(fvp-aemv8r-smp)` and needs a Zephyr 3.7 workspace plus the
Arm FVP to build, and acceptance for a migration here is a build.

This is **not** a class called "hand-written entries". A shape that exists only
because a generator is missing must not get a name that makes it look
intentional — it is class 1z before 1288, and it disappears when 1288 closes.

Recount both numbers with:

```sh
git ls-files ':(glob)examples/workspaces/*/src/*entry*/CMakeLists.txt'   # the 1z packages
grep -rn 'entry *=' examples/workspaces/*/src/*_bringup/system.toml      # the rows naming one
```

## Naming rules

| rule | why |
|---|---|
| **No language prefix** in a single-language workspace (`talker_pkg`, not `c_talker_pkg`) | the directory already names the language. Prefixes are kept in `mixed` and `features`, where languages coexist and the prefix carries information |
| **Roles, not payloads** — `service_server_pkg`, not `add_server_pkg` | AddTwoInts is what the demo sends; the ROLE is what is being compared across languages |
| **One platform vocabulary** for entries — `freertos`, `nuttx`, `threadx`, `zephyr`, `esp32` | no `qemu_` or `native_` qualifiers; the board is named once, the same way everywhere. Since phase-445 W5 the rule governs the `[image.<id>]` **id**, because the builder generates `<id>_entry` from it — a reader looking for a `freertos_entry` *directory* will not find one, and should not |
| **Node names and executables are NOT normalised** | a node name is the ROS wire identity: it appears in `ros2 node list`, in resolved models, and in test expectations. `add_server` / `fib_server` stay, and c and cpp already agree on them |

Check the **package** invariant with:

```sh
diff <(ls examples/workspaces/c/src) <(ls examples/workspaces/rust/src)
```

Only genuine coverage differences should appear — never a naming difference.
Since phase-445 W5 that command compares node packages and bringups **plus the
surviving class-1z entry packages**; it can no longer see the other entries at
all, because they are image rows.

## Known coverage differences

Real gaps, not naming drift; each a candidate for closing. **These are
`[image.*]` ROWS, not directories** — the earlier version of this table named
them `…_entry` and sent readers looking for packages that phase-445 W5 had
already replaced with generated ones.

Re-derive rather than trusting the table:

```sh
for w in rust c cpp; do
  printf '%s: ' "$w"
  grep -oE '^\[image\.[a-z0-9_]+\]' "examples/workspaces/$w/src/demo_bringup/system.toml" \
    | tr -d '[]' | sed 's/image\.//' | sort | tr '\n' ' '
  echo
done
```

Measured 2026-09-27 — rust 17 rows, c 15, cpp 16, **12 common to all three**:

| present in | missing from | what it is |
|---|---|---|
| rust | c, cpp | `esp32` — the ESP32-C3 board build |
| rust | c, cpp | `native_service_inprocess` — same-process service round-trip |
| rust | c, cpp | `native_showcase` — the combined showcase launch |
| rust | c, cpp | `zephyr_robot1` — the per-host Zephyr multi-host image (class 1z) |
| rust, c | cpp | `nuttx` — the NuttX board build |
| c, cpp | rust | `freertos_posix` — the `freertos-posix` board (Cyclone DDS) |
| c | rust, cpp | `riscv_threadx` — the `rv-virt-threadx` board (ThreadX on QEMU RISC-V 64) |
| cpp | rust, c | `s32z270`, `mps3_an536` — two further FreeRTOS boards (`s32z270-freertos`, `mps3-an536-freertos`), Cyclone DDS |
| cpp | rust, c | `zephyr_cyclonedds` — the Cyclone DDS Zephyr image (class 1z) |

`mixed` is deliberately different: it is the language SEAM (one entry, components
from several languages), so it keeps language prefixes and does not mirror the
node set.

`features` holds the capability demos (params, lifecycle, QoS, custom messages,
remap) for all three languages and is **native only** — `param_services` and
`lifecycle` are alloc-gated, and an embedded image must opt into them
explicitly, so keeping them here leaves the language workspaces' embedded
entries clean.
