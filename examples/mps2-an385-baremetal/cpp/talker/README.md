# Bare-metal C++ talker (MPS2-AN385, no RTOS)

Publishes `std_msgs/String` `"Hello World: N"` on `/chatter` once a second from
an ordinary nano-ros **C++** component, on a Cortex-M3 with **no RTOS at all**.

Same shape as [`../../c/talker`](../../c/talker/README.md), whose README
explains the layout and how the image boots:

| Path | What it is |
| --- | --- |
| `src/baremetal_cpp_talker/` | the component: `CMakeLists.txt` (`nano_ros_add_node`), `include/baremetal_cpp_talker/Talker.hpp`, `src/talker.cpp`, `package.xml`, `nros-codegen.toml` |
| `src/baremetal_cpp_talker_bringup/` | `system.toml`, `board-net.toml`, `system.contract.yaml` |

`Talker` is a typed component: the generated entry constructs it and calls
`configure(node)`, which creates the publisher and binds `on_tick` as a 1 Hz
timer. It names no board.

C++ on this board builds with `-fno-exceptions -fno-rtti`, and the board's C
start runs the static constructors before the entry, so a namespace-scope C++
object is constructed before it is used.

`nros-codegen.toml` bounds `std_msgs/String.data` (256).

## Build and run

```sh
source ./activate.sh
cd examples/mps2-an385-baremetal/cpp/talker
nros build
```

The image dials `tcp/10.0.2.2:10400`. The fixture lane builds it from its
`[[workspace_fixture]]` row (`builder = "cmake"`), and
`test_qemu_baremetal_capi_e2e` pairs it with `../listener`.
