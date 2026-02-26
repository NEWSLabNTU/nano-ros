# nros C API {#mainpage}

Lightweight ROS 2 client library for embedded real-time systems.

## Quick Start

```c
#include <nros/nros.h>

nros_node_t node;
nros_node_init(&node, "my_node", "");

nros_publisher_t pub;
nros_publisher_init(&pub, &node, "chatter",
                    my_msg__serialize, my_msg__deserialize);

MyMsg msg = { .data = "hello" };
nros_publisher_publish(&pub, &msg);

nros_publisher_fini(&pub);
nros_node_fini(&node);
```

## API Modules

| Header | Description |
|--------|-------------|
| @ref node.h | Node creation and lifecycle |
| @ref publisher.h | Topic publishers |
| @ref subscription.h | Topic subscribers |
| @ref service.h | Service servers |
| @ref client.h | Service clients |
| @ref action.h | Action servers and clients |
| @ref executor.h | Callback executor (polling) |
| @ref timer.h | Periodic timers |
| @ref guard_condition.h | Manual wake-up triggers |
| @ref lifecycle.h | Node lifecycle state machine |
| @ref parameter.h | Parameter services |
| @ref cdr.h | CDR serialization helpers |
| @ref clock.h | Clock and time types |
| @ref init.h | Library initialisation |

## Header Organisation

Each per-module header above is the authoritative C API surface for its
module, with hand-written Doxygen documentation.  Include individual
headers for what you need, or use `<nros/nros.h>` for everything.

Shared types (return codes, QoS, time) live in `<nros/types.h>`, which
all module headers include automatically.

An internal `nros_generated.h` (produced by cbindgen) is used for
compile-time drift detection — it is not part of the public API.
