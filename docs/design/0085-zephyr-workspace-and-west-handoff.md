# RFC-0085 — Where the nano-ros workspace lives, and who calls whom on Zephyr

**Status:** Draft (2026-08-29)
**Amends / refines:** RFC-0065 (`nros build`) — D5's framework-entry escape, and
the west driver added for issue 0892.
**Motivated by:** issue 0892 — `nros build`'s west driver could not build any
Zephyr image, and fixing it surfaced two questions the tree had never answered
because its examples hide them.

## The distinction the examples hide

Two different things are both called "nano-ros" in a Zephyr build, and in this
repository they are the same directory, which is why the coupling has never had
to be stated:

| | what it is | where it must be |
| --- | --- | --- |
| **the framework** | this repo; `zephyr/` inside it is the Zephyr module | a west PROJECT in the west workspace |
| **the user's workspace** | their `src/*` packages, `system.toml`, `[image.*]` | see D1 |

`zephyr/CMakeLists.txt` sets `NROS_REPO_DIR = ${CMAKE_CURRENT_LIST_DIR}/..`, so
the module locates the FRAMEWORK by walking up out of itself. Nothing locates
the user's workspace, because in every in-tree example it is the same checkout.
A real user has two trees, and the build has no way to say so.

## D1 — The user's workspace lives OUTSIDE the west workspace, linked by path

A nano-ros workspace is platform-agnostic by construction: one `system.toml`
declares `[image.native]`, `[image.freertos]`, `[image.nuttx]` and
`[image.zephyr]` side by side, and RFC-0065 D6 makes the image the buildable
unit precisely so those coexist. A west workspace is Zephyr-specific: `west
update` populates it with Zephyr, its modules and their revisions.

Nesting the first inside the second gets the dependency backwards:

* a workspace targeting only native and FreeRTOS would need a west workspace to
  exist for nothing;
* `west update` would have opinions about the user's application code, which is
  not what a manifest is for;
* the workspace's other platforms would be built from inside a tree whose
  toolchain environment is Zephyr's.

Zephyr already supports this, and it is **measured, not assumed**:

```
$ (unset ZEPHYR_BASE; west build --help)
west: unknown command "build"; do you need to run this inside a workspace?

$ ZEPHYR_BASE=…/zephyr-workspace/zephyr west build --help
usage: west build [-h] [-b BOARD[@REV]] …
```

`ZEPHYR_BASE` alone makes `west build` runnable from ANY directory. There is no
`.west/` requirement — which is exactly what a **freestanding application**
(one outside the west workspace) relies on, and it is how every Zephyr fixture
in this tree already builds: `scripts/build/west-fixtures.sh` runs `west` from
the repo root with `ZEPHYR_BASE` exported, against apps under `examples/` that
sit outside `zephyr-workspace/`.

So: **the framework is a west project; the user's workspace is not.** The link
is a path, resolved rather than assumed — which is what issue 0892's fix
implemented from the workspace side:

```
$ZEPHYR_BASE  →  $NROS_ZEPHYR_WORKSPACE  →  <workspace>/zephyr-workspace
              →  ../nano-ros-workspace[-4.4]
```

That ladder is `west-fixtures.sh`'s, reused rather than reinvented, and
`NROS_ZEPHYR_WORKSPACE` is the established spelling — 60 references across the
tree. The first version of 0892's fix searched for a `.west/` directory under a
new `NROS_WEST_WORKSPACE`, which was both a 61st name for one thing and the
wrong CONDITION: it would have refused this repository's own layout.

**The missing half is the other direction.** A west build knows where the
FRAMEWORK is (`NROS_REPO_DIR`) and not where the user's WORKSPACE is. Anything
the workspace owns — generated message crates, the resolved SystemModel, the
per-build sizes headers, the entry's own crate — is therefore unreachable from a
west build of a freestanding app. That is the concrete gap D2 has to close.

## D2 — West calls the workspace build; `nros build` does not call itself

**West stays the driver for the final image.** It links the Zephyr kernel, it
owns the Kconfig, it produces the ELF. Nothing about nano-ros should displace
that, and a user must be able to type `west build -b <board> <app>` and have it
work — the book says so and it stays true.

The workspace build is therefore a **supplier**, invoked BY west, delivering the
artifacts west links. This is not new: `zephyr/cmake/nros_cargo_build.cmake`
already does exactly this shape —

```cmake
add_custom_target(${_target_name}_build
    COMMAND ${CMAKE_COMMAND} -E env <kconfig-derived env> cargo ${CARGO_ARGS}
    BYPRODUCTS ${_cargo_byproducts})
add_library(${_target_name} STATIC IMPORTED GLOBAL)   # west links this
```

— west's configure emits a target that runs a cargo build with Kconfig-derived
environment, and imports the resulting staticlib. The pattern is right. What is
wrong is that it assembles the cargo invocation by hand, so a Zephyr build and
an `nros build` of the same image are two different derivations of the same
thing, and only one of them knows about `[image.*]`.

### The recursion this must avoid

`nros build <zephyr image>` runs `west build` (issue 0892). If west's configure
ran `nros build`, that is a loop. So the supplier entry point must NOT be the
same verb:

| caller | verb | does |
| --- | --- | --- |
| the user | `nros build <zephyr image>` | resolve app + workspace, hand off to `west build` |
| west's configure | a supplier verb (name TBD) | sync, codegen, models, staticlibs — **never** invokes west |

The supplier is roughly today's `nros sync` plus the cargo targets, addressed by
IMAGE so it can apply that image's features and knobs. `nros sync` already
covers the codegen half.

### What the supplier must deliver

Everything a west build cannot derive for itself, all of it owned by the
workspace rather than the framework:

* generated message crates (`nros sync`);
* the resolved SystemModel the entry macro bakes;
* the per-build sizes headers (`nros_{,cpp_}config_generated.h`) — the family of
  issues 0088 → 0834, which exists precisely because this artifact crosses the
  boundary between a cargo build and a CMake build;
* the entry staticlib itself.

## D3 — No Zephyr-specific verb; the platform difference lives in config

`nros build` must not grow a Zephyr-shaped verb. Platform differences are
already declarative everywhere else in this design — a bringup declares its
system, an `[image.*]` declares its board and overlays, an entry declares its
deploy — and a verb per framework is the shape that stops those declarations
being the single answer.

So the supplier D2 needs is NOT `nros stage-for-zephyr`. Two options remain,
and both keep the platform knowledge in data:

* **a west extension.** The mechanism already ships: `scripts/west-commands.yml`
  registers `west fvp` via `zephyr/module.yml`'s `west-commands:` key, so any
  workspace listing nano-ros as a project picks the command up with no `west
  config` step. A `west nros-build`-style extension is a natural home for
  "prepare this image's artifacts", and it lives on the WEST side of the
  boundary where the Zephyr-specific knowledge belongs.
* **an image declaration consumed by the module's CMake.** The module already
  reads Kconfig and derives cargo invocations (`nros_cargo_build.cmake`); what
  it lacks is the workspace path and the image identity. Both are data, and both
  could arrive as cache variables the app's `CMakeLists.txt` sets from its own
  location.

The second needs no new command at all, which is why it is the one to try
first.

## What this RFC does not decide

* **Which of D3's two shapes the supplier takes.** The CMake-variable one adds
  no command and is the one to try first; the west extension is the fallback if
  the module genuinely needs to invoke a workspace-side tool.
* **How west learns the workspace path.** A `-DNROS_WORKSPACE=<dir>` on the
  west command line is the obvious first answer, but the app's `CMakeLists.txt`
  could equally derive it from its own location — the entry package IS in the
  workspace. The second needs no user action and is probably right; it is not
  yet checked against a freestanding app.
* **The harness fields.** `west_build_name`, `west_id` and
  `west_zenoh_locator` in `examples/fixtures.toml` are per-fixture concerns (a
  private build dir and locator so parallel legs do not collide), not image
  ones. Whether they become image keys, stay row keys, or move into the
  handoff's native args is what currently blocks phase-383 W9.b's last 14 rows.

## D4 — the application is named by its own DEPLOY declaration, in whichever
## file its language uses; ambiguity is an error

**Decided 2026-08-29, by running the flow rather than reasoning about it.**

An image resolves to an application package. The link is the package's own
declaration of the deploy target it serves, and the two languages put it in
different files:

| language | where |
| --- | --- |
| Rust  | `[package.metadata.nros.entry] deploy = "zephyr"` in `Cargo.toml` |
| C/C++ | `nano_ros_add_executable(... DEPLOY zephyr)` in `CMakeLists.txt` |

Reading only the first was a **silent Rust-only restriction**. The `c`, `cpp`,
`mixed`, `realtime-c` and `realtime-cpp` workspaces each have a `zephyr_entry`
declaring `DEPLOY zephyr` and no `Cargo.toml` for it, so all five fell through
to the fallback — the bringup directory. That is a real directory, so nothing
errored; west was simply pointed at the wrong tree, and the first symptom was a
conf fragment reported "not found" in two paths that were the same path twice.

**Ambiguity is refused, not resolved by order.** Six of the fourteen Zephyr
images match more than one entry package: `realtime-cpp` has `zephyr_entry` and
`fvp_entry` (both `DEPLOY zephyr`, same board, different payloads),
`examples/workspaces/rust` has `zephyr_entry` and `zephyr_entry_robot1`, and
`features` has three. A first-match scan returns a right-looking answer in every
one of those cases and a WRONG one in some — `[image.zephyr_robot1]` would have
built `zephyr_entry`. So the resolver lists the candidates and stops, and the
image says which:

```toml
[image.zephyr_robot1]
entry = "zephyr_entry_robot1"
```

This is why `entry` is an image key rather than a derived value, and it is the
only new key this RFC adds.

## D5 — a fragment is searched beside the APPLICATION, not only the bringup

Corollary of D2 that only appeared once the driver stopped assuming the bringup
IS the application. A Zephyr app keeps its `prj-<rmw>.conf` next to the
`CMakeLists.txt` west builds, so the fragment search is board config dir →
application → bringup. Before this, an image naming a fragment that lived with
its app got

```text
conf fragment `prj-zenoh.conf` not found. Looked in:
  …/src/demo_bringup/boards/native_sim_native_64/prj-zenoh.conf
  …/src/demo_bringup/prj-zenoh.conf
```

while the file sat in `src/zephyr_entry/` the whole time. All 14 Zephyr images
now declare their RMW overlay, which is what their hand-written CMakeLists
already required (`FATAL_ERROR "… requires an RMW overlay"`) and what the
fixture rows already said as `conf_files`.

## Verified end to end

`examples/workspaces/rust`, 2026-08-29, on this tree:

```text
nros sync
nros build demo_bringup:zephyr        → west build → 1312/1312 → zephyr.exe
just zenohd                           → rmw_zenohd on tcp/127.0.0.1:7447
./build/zephyr/zephyr.exe             → "zephyr workspace entry up (2 nodes)"
ros2 topic echo /chatter --once       → data: 6
```

`examples/workspaces/c` builds the same way (1326/1326) through the D4 CMake
rung; its runtime needs a `zeth` TAP interface, which is host setup requiring
root, not part of this flow.

**One defect this surfaced, now fixed.** `zephyr/Kconfig`'s
`NROS_ZENOH_LOCATOR` defaulted to `tcp/127.0.0.1:7456`, while
`scripts/dev/zenohd.sh` (`just zenohd`, `nros_router_hint`) and `rmw_zenoh_cpp`
itself all use `7447`. The two halves of our own documented
workflow disagreed, and neither error message names a port — the image says
`Transport(ConnectionFailed)` and a `ros2` node on the other router says
"Unable to connect to a Zenoh router". Measured before changing it: of 35 built
Zephyr images carrying a locator, 34 set their own (fixtures allocate one) and
1 rode the default.

## Evidence

* `zephyr/CMakeLists.txt:68` — `NROS_REPO_DIR = ${CMAKE_CURRENT_LIST_DIR}/..`,
  the framework locating itself; nothing equivalent for the workspace.
* `zephyr/cmake/nros_cargo_build.cmake:600–634` — the existing
  custom-target-plus-IMPORTED-library supplier shape.
* `book/src/getting-started/integration-zephyr.md` — the documented layout
  (`my_zephyr_ws/{.west,zephyr,modules/nano-ros,apps/my_app}`) and the
  `nros build` handoff added for 0892.
* issue 0892 — why the driver could not work, and the three-rung workspace
  resolution.
