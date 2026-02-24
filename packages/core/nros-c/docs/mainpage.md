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

All type definitions and function declarations are generated into
`nros_generated.h` by cbindgen. The per-module headers above are thin
wrappers that `#include "nros/nros_generated.h"` for convenience.
