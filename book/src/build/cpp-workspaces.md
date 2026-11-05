# C / C++ multi-node workspaces

[Growing your workspace](growing-your-workspace.md) describes the canonical
node pkgs + bringup pkg + `[image.*]` shape against the Rust path
(`nros::node!(…)` + `nros::main!(launch = …)`). This chapter shows the
**C and C++ path** through the same shape, role-for-role.

The parity gap is closed. Same launch.xml, same `package.xml`,
same `system.toml`, same workspace pkg-index — the only thing that
changes language-side is the cmake-fn / macro surface.

## TL;DR — side-by-side

| Role | Rust | C / C++ |
|---|---|---|
| **Node pkg** | `lib.rs` with `nros::node!(MyNode)`, declared by a `[[component]]` row in the bringup's `system.toml` | `Talker.{hpp,cpp}` with a `configure(::rclcpp::Node&)` component method (C++) / `RCLC_C_COMPONENT` (C); `CMakeLists.txt` calling `nano_ros_auto_add_library` + `nros_components_register_node` |
| **Bringup pkg** | `package.xml` + `system.toml`, optionally `launch/*.launch.xml` (no `Cargo.toml`) | identical (language-agnostic) |
| **Image** | `[image.linux] board = "linux"` in the bringup's `system.toml` | identical (language-agnostic) |
| **The entry** | GENERATED into `build/<coord>/linux_entry/` — a `Cargo.toml` and a `nros::main!(launch = "demo_bringup")` | GENERATED into `build/<coord>/CMakeLists.txt` — a `nano_ros_add_executable(linux_entry BOARD … BRINGUP … LAUNCH … LANG cpp TYPED DEPLOY linux)` call, plus the `main` TU the verb emits |
| **Workspace root** | none — the generated entry's own `Cargo.toml` is the cargo root | none — the generated `build/<coord>/CMakeLists.txt` is the cmake root, calling `nano_ros_workspace(WORKSPACE_ROOT … BACKEND zenoh PLATFORM posix SYSTEM demo_bringup SUBDIRS …)` |
| **Build** | `nros build linux` | `nros build linux` |
| **Run** | `nros run linux` (binary: `build/posix-zenoh/linux_entry/target/debug/linux_entry`) | `nros run linux` (binary: `build/posix-zenoh-linux/cmake/linux_entry`) |

The reference C++ workspace ships in-tree at
[`examples/workspaces/cpp/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/cpp).
Copy the whole directory, rename the packages.

## Workspace layout

Identical structure to the Rust template, swapping `Cargo.toml` →
`CMakeLists.txt` inside each Node pkg — and, at the root, nothing at all:

```text
my_ws/
├── .colcon_workspace             # tracked marker; there is no root build file
└── src/
    ├── talker_pkg/               # Node pkg (C++)
    │   ├── package.xml
    │   ├── CMakeLists.txt        # nano_ros_auto_add_library + nros_components_register_node
    │   └── src/{Talker.hpp,Talker.cpp}
    ├── listener_pkg/             # Node pkg (C++)
    │   ├── package.xml
    │   ├── CMakeLists.txt
    │   └── src/{Listener.hpp,Listener.cpp}
    └── demo_bringup/             # Bringup pkg (language-agnostic — copy/paste
        ├── package.xml           #          works between Rust and C++ workspaces)
        ├── system.toml           #          [system] + [[component]] + [image.*]
        └── launch/system.launch.xml
```

No entry package, and nothing at the root: a workspace is a directory of
packages. `nros build <image>` writes a `CMakeLists.txt` and the
entry's `nano_ros_add_executable(…)` call into `build/<coordinate>/`, configures
that into `build/<coordinate>/cmake/`, and builds — see
[What `nros build` generates](growing-your-workspace.md#what-nros-build-generates). The exception is a Zephyr application,
which keeps a package because west needs a real application directory.

## The generated cmake root

You do not write this; `nros build` does, from the packages it discovered and
the image it resolved. It is the cmake root — and it lives in
`build/<coordinate>/`, not at the workspace root, which is why its first
argument says where the workspace actually is. It is worth reading once,
because everything below happens inside it:

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_ws LANGUAGES C CXX)
find_package(nano_ros REQUIRED COMPONENTS workspace)

nano_ros_workspace(
    WORKSPACE_ROOT "../.."               # this file lives in build/<coord>/
    BACKEND        zenoh                 # zenoh | xrce | cyclonedds
    PLATFORM       "posix"
    SYSTEM         demo_bringup
    ORDER_FROM_DEPENDS
    SUBDIRS        "../../src/talker_pkg"
                   "../../src/listener_pkg"
)
```

`nano_ros_workspace()` does the heavy lifting in one call:

1. Sets `NROS_PLATFORM=posix` + `NROS_RMW=zenoh`.
2. `add_subdirectory(<nano-ros>)` **once** at root scope (so per-pkg
   subdirs don't collide on re-include).
3. `include(NanoRosNodeRegister.cmake)` + `include(NanoRosEntry.cmake)`
   once.
4. `add_subdirectory(<each member>)` for each `SUBDIRS` entry.

Subdir CMakeLists begin with the dual call:

```cmake
nano_ros_workspace_pkg_guard()  # no-op inside a workspace; bootstraps standalone solo
```

— the cmake equivalent of cargo `[workspace]` discipline. Every member
compiles standalone (with `-Dnano_ros_ROOT=<sdk-root>`, or a `cmake --preset`
from `nros setup presets`) or as part of the
workspace; the per-pkg CMakeLists doesn't change between modes.

## Node pkg

A **typed component** — no `main()`. The pkg ships a class with a
`configure(::rclcpp::Node&)` method that creates real entities (a `Publisher`, a
`Timer`) and binds member callbacks **by identity** (member-fn-pointer template
param, no string callback name, no interpreter). The generated entry constructs
the object and calls `configure(node)`; the executor dispatches the callbacks.

```cmake
# src/talker_pkg/CMakeLists.txt
cmake_minimum_required(VERSION 3.22)
project(talker_pkg LANGUAGES C CXX)
nano_ros_workspace_pkg_guard()
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# The ament shape — the library owns the sources, the register
# owns the identity (keyword parity with rclcpp_components_register_node).
# Interface deps (std_msgs__nano_ros_cpp) are wired automatically.
nano_ros_auto_add_library(talker_lib STATIC src/Talker.cpp)

nros_components_register_node(talker_lib
    PLUGIN talker_pkg::Talker      # any qualified name — upstream namespaces port verbatim
    EXECUTABLE talker
    SHAPE configure)               # this walkthrough uses the configure(Node&) shape
```

### Telling the build what the component creates

`NROS_EXECUTOR_MAX_CBS`, the subscriber slots and the arena are `const` sizes
compiled into nros-node before a component TU is compiled, while a typed
component wires itself in its CONSTRUCTOR, at runtime. Nothing the macros emit
can supply those sizes — it exists only after linking. So somebody has to state
what the image creates, ahead of the build.

That statement is a **contract sidecar** beside the launch file, not something
in this CMakeLists. `<stem>.contract.yaml` next to `<stem>.launch.xml` in the
bringup package:

```yaml
# src/demo_bringup/launch/system.contract.yaml
version: 1

nodes:
  talker:
    paths:
      on_timer:                                  # a path with a timer trigger
        trigger: { timer: { rate_hz: 1 } }       # IS the periodic callback
        output: [chatter]
    pub:
      chatter: {}
  listener:
    sub:
      chatter: {}

topics:
  /chatter:
    type: std_msgs/msg/Int32
    pub: [talker/chatter]
    sub: [listener/chatter]
```

`nodes:` keys are LOCAL endpoint names — an absolute path there is concatenated
onto the node's FQN and matches no topic. `topics:`, `services:` and `actions:`
wire those endpoints to absolute names and carry the type. `paths:` is where
timers live: the model records a path with no `input` as the periodic callback.

**A bringup with no `launch/` writes the same file at `<bringup>/system.contract.yaml`**,
beside its `system.toml`. Its launch file is generated from the `[[component]]`
rows, so there is no `launch/` directory to put a sidecar in; the build carries
this one to where the resolver looks. That is how a one-node bringup states its
QoS — and stating it is worth real memory: one `sub: { chatter: { qos: { history:
keep_last, depth: 1 } } }` row takes a one-node Rust listener on Linux from 273,802
to 172,298 bytes of static RAM, because the zenoh backend then sizes its payload
pools for the one sample the image keeps instead of the four it might have.

The name you write must be the name the code registers, ABSOLUTELY. A contract
states RESOLVED names; a call site writing `"chatter"` or a node the launch file
remaps is one the build cannot match with certainty, so it refuses that endpoint
by name and the image keeps its configured pools rather than being sized from a
guess.

The build folds it into the resolved SystemModel, and the entry passes that
model to the sizing automatically — nothing here takes an argument for it.

The derivation is all-or-nothing per image: **a component the contract does not
describe makes that image derive nothing** and keep its configured
`NROS_EXECUTOR_MAX_CBS`. A count taken over only the components that answered
is smaller than the image needs, and a short `MAX_CBS` fails entity creation at
boot — so "some of them" is refused rather than guessed at.

Note that a publisher is inventoried but claims no callback slot, so the entity
count and the derived `MAX_CBS` are legitimately different numbers.

> Earlier versions stated this per component, as
> `nano_ros_node_register(ENTITIES ...)`. That keyword is retired
> and now fails with a pointer here. It was a list beside the code with nothing
> comparing the two, and on the safety island one of them declared six
> subscriptions for a node that creates seven: every pool derived from it was
> short by one, and the eleventh subscription failed at boot with a transport
> error that named nothing.

#### The census: the contract checked against the code

Since the contract is still a statement beside the code, a CROSS image (any
image whose board is not `linux`) checks it before it configures: the host
image generated from the same launch file is run once in census mode — it
constructs every component against a recording backend, writes what they
created, and exits without spinning or dialling a router — and the configure
compares that census with the contract row by row. An endpoint the code creates
and the contract omits refuses as `missing-in-contract` (not waivable: it is
the short pool); one the contract declares and the code never creates refuses
as `phantom` (waivable per row under `[census.waive]`).

The configure never builds the native image itself. `nros build` does: before
it plans a cross image, it takes that image's census. Taking it does nothing
when there is nothing to do — a model with no contract, or a census that still
matches the code (freshness is by content: touching a file changes nothing,
adding a subscription does). A build driven some other way (a hand-run
`west build` or `cmake`) with no census, or a stale one, REFUSES at configure
and prints the command that takes it; running `nros build` once for the image
takes it too. A bringup that wants the old
behaviour writes `[census] on_missing = "warn"` / `on_stale = "warn"` in its
`system.toml`, and gets a CMake WARNING instead.

```cpp
// src/talker_pkg/include/talker_pkg/Talker.hpp
#pragma once
#include <rclcpp/rclcpp.hpp>
#include "std_msgs.hpp"

namespace talker_pkg {

class Talker {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    ::rclcpp::Timer timer_;
    int count_ = 0;

    void on_tick();  // real body; bound via &Talker::on_tick (no name)

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

}  // namespace talker_pkg
```

```cpp
// src/talker_pkg/src/Talker.cpp
#include "talker_pkg/Talker.hpp"

namespace talker_pkg {

void Talker::on_tick() {
    std_msgs::msg::Int32 m;
    m.data = count_++;
    (void)pub_.publish(m);
}

::rclcpp::Result Talker::configure(::rclcpp::Node& node) {
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    // Member-fn-pointer-as-template-param → no-alloc trampoline; `this` is ctx.
    return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
}

}  // namespace talker_pkg
```

The generated entry constructs `Talker` in static storage and calls
`configure(node)` on the real executor — the same component model the Rust `nros::node!(Talker)` +
the C `RCLC_C_COMPONENT` paths use, so C++, C, and Rust Node pkgs interoperate in
one launch graph.

Scaffold a C++ Node pkg with:

```bash
nros new my_talker --lang cpp --template talker --to demo_bringup
```

From the workspace root this adds `src/my_talker/` (class
`my_talker::Talker`) and, with `--to`, its `[[component]]` row in
`demo_bringup`.

## Bringup pkg

Language-agnostic — copy verbatim from the
[bringup steps](growing-your-workspace.md#2-add-a-component-row). `package.xml` +
`system.toml`, and `launch/system.launch.xml` when you want one (without it the
launch file is generated from the `[[component]]` rows). No `Cargo.toml`, no
`CMakeLists.txt`. Stock ROS 2 launch.xml from nav2 / Autoware /
turtlebot3 pastes in modulo unsupported tags.

```xml
<!-- src/demo_bringup/launch/system.launch.xml -->
<launch>
  <node pkg="talker_pkg" exec="talker" name="talker"/>
  <node pkg="listener_pkg" exec="listener" name="listener"/>
</launch>
```

## The image, and the entry it generates

Declare the program you want in the bringup's `system.toml` —

```toml
[image_defaults]
rmw = "zenoh"

[image.linux]
board = "linux"
```

— and `nros build linux` emits this into that generated root. `BOARD` and
`DEPLOY` come from the resolved image, so a generated entry cannot disagree
with the board it was generated for; there is no `SOURCES`, because the verb
generates the translation unit carrying `main`:

```cmake
nano_ros_add_executable(linux_entry
    BOARD   native
    BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../../src/demo_bringup"
    LAUNCH  default
    LANG    cpp
    TYPED
    DEPLOY  native)
```

At configure time the cmake fn asks the `nros` CLI to generate the entry and
emits `${CMAKE_BINARY_DIR}/linux_entry_nros_main_generated.cpp` (the canonical
`int main()` body that constructs each launch node's component + calls
`configure(node)` on the real executor via `LinuxBoard::run_components`),
appends it to the target's sources, and auto-links every
`<pkg>_<exec>_component` static lib the launch XML named.

### `BRINGUP` + `LAUNCH` — name your input, never the artifact

`BRINGUP` + `LAUNCH` name your *input*: the Bringup package and a launch
selector. The build resolves that into a SystemModel build artifact (under
`<ws>/build/nros/models/<bringup>/` — never committed; gate
`check-no-tracked-models`), and the entry bakes from it, so the same artifact
drives the Linux runtime and every embedded image (contracts, tiers, QoS never
drift). `MODEL` — naming the resolved artifact file directly — is a deprecated
expert override; the two are mutually exclusive, and a *committed* model is
banned outright.

`nros build` re-resolves the model whenever the bringup's launch XML or
`system.toml` changed; with nothing changed it costs a digest comparison.

### Writing the entry yourself

One case needs a hand-written one: **startup the declarations cannot
express.** `nros materialize <bringup>:<image>` writes the generated entry out to
`src/<image>_entry/`, as a package with `nano_ros_entry(NAME … )` in its own
`CMakeLists.txt`, and `nros build` then leaves it alone. (A Rust image has a
smaller step first, `[image.*] before_boot`; the C/C++ entry has no such hook
yet.)

A Zephyr image is not a second case. `nros build` and `nros run` generate its
Zephyr application under `build/` from the image, like every other entry, and
`nros materialize` is the way to own it there too
([the Zephyr chapter](../targets/zephyr.md#owning-the-application-nros-materialize)).

The materialized package needs no source file of its own: the cmake function
generates the whole body (the `main` above), so its `CMakeLists.txt` names the
bringup and launch file and nothing else.

## Build + boot

```bash
nros build linux
nros run linux        # or run ./build/posix-zenoh-linux/cmake/linux_entry
```

`nros build` prepares everything first: the generated message bindings, the
SystemModel (via the pinned `nros-launch-resolve` helper), and
`build/<coordinate>/CMakeLists.txt`. Then it configures and builds. Each piece
is redone only when its inputs changed, so there is no step to run before it.
The coordinate directory names what it contains: platform, RMW, board.

Driving `cmake` yourself against that generated root —
`build/<coordinate>/CMakeLists.txt`, never a file at the workspace root — still
works. Run `nros configure` first to write it without building;
`nros build linux --dry-run` prints the
exact `cmake` lines.

The build produces:

- `src/talker_pkg/libtalker_pkg_talker_component.a` — the Node pkg static lib
  (`_component` is the compatibility target suffix).
- `src/listener_pkg/liblistener_pkg_listener_component.a` — ditto.
- `linux_entry` — the image's executable, with the generated
  `int main()` + register-call sequence + Board boot stub linked in.

`cmake configure` is incremental — pinned `CMAKE_CONFIGURE_DEPENDS` on
every file the entry generator reads (depfile from the CLI), so any
launch.xml / `package.xml` / Node pkg edit re-runs codegen.

## What runs where

| Concern | Lives in |
|---|---|
| Node entities + real callbacks | Node pkg `configure(::rclcpp::Node&)` |
| Topology + launch args + per-target deploy | Bringup pkg `system.toml` + `launch/*.launch.xml` |
| `int main()` + executor init + spin | Generated TU emitted by the entry's cmake fn |
| Board + RMW selection | The image's `board` and `rmw`, which become the generated `BOARD` / `DEPLOY` args |

Same partition as the Rust track — the only thing that changes is the
syntax the user types into the two package roles.

## C / C++ scaffolding via `nros new`

| Command | Output |
|---|---|
| `nros new <ws> --board <board> --template talker --lang cpp` | A new workspace: a C++ Node pkg `<x>_node` plus its Bringup pkg `<x>_node_bringup` with one image |
| `nros new <name> --lang cpp --template talker [--to <bringup>]` | Inside a workspace: a C++ Node pkg under `src/`; `--to` adds its `[[component]]` row |
| `nros new system <name>_bringup --components a,b` | Bringup pkg (language-agnostic — works for both Rust and C++ workspaces) |
| `nros materialize <bringup>:<image>` | The image's generated entry (for a Zephyr image, its west application) copied to `src/<image>_entry/` as a package you own |

`--lang c` scaffolds a pure-C Node pkg the same way. Pure-C and mixed C/C++ workspace examples
live under [`examples/workspaces`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces).

## See also

- [`examples/workspaces/cpp/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/cpp)
  — the canonical reference workspace, all C++: the talker, listener,
  service and action Node pkgs, and one bringup per behaviour
  (`pubsub_bringup`, `service_bringup`, `action_bringup`) whose image rows
  are the paired examples, one per platform and role.
- [C++ Entry-pkg roadmap doc](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/archived/phase-219-cpp-entry-pkg.md)
  — full landing order + acceptance bar.
- [Multi-node workspace layout design](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0024-multi-node-workspace-layout.md)
  §11 — LOCKED canonical shape (`Bringup + Node + Entry`).

## Design background

- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md): behaviour bringups, single-node witnesses, and cadence-backed CI.
  The paired examples as image rows of behaviour bringups in the language
  workspaces.
