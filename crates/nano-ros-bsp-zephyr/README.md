# nano-ros-bsp-zephyr

Board Support Package for running nano-ros on Zephyr RTOS.

This library provides a simplified C API that abstracts away zenoh-pico configuration and provides ROS 2 compatible topic naming.

## Features

- **Zero-config via Kconfig**: Configure zenoh locator, domain ID at build time
- **ROS 2 compatible**: Automatic keyexpr formatting for ROS 2 interop
- **Simple API**: Create nodes, publishers, subscribers with minimal code
- **~70% code reduction**: Examples reduced from 130+ lines to ~40 lines

## Usage

```c
#include <nano_ros_bsp_zephyr.h>

void main(void) {
    // Initialize BSP (uses CONFIG_NANO_ROS_ZENOH_LOCATOR from Kconfig)
    nano_ros_bsp_context_t ctx;
    nano_ros_bsp_init(&ctx);

    // Create node
    nano_ros_node_t node;
    nano_ros_bsp_create_node(&ctx, &node, "my_talker");

    // Create publisher
    nano_ros_publisher_t pub;
    nano_ros_bsp_create_publisher(&node, &pub, "/chatter", "std_msgs::msg::dds_::Int32_");

    // Publish messages
    uint8_t buffer[64];
    int32_t count = 0;

    while (1) {
        count++;
        int32_t len = serialize_int32(count, buffer, sizeof(buffer));
        nano_ros_bsp_publish(&pub, buffer, len);
        nano_ros_bsp_spin_once(&ctx, K_SECONDS(1));
    }
}
```

## Configuration (prj.conf)

```ini
# Enable nano-ros BSP
CONFIG_NANO_ROS_BSP=y

# Zenoh router address
CONFIG_NANO_ROS_ZENOH_LOCATOR="tcp/192.168.1.1:7447"

# ROS 2 domain ID
CONFIG_NANO_ROS_DOMAIN_ID=0

# Startup delay (ms) for network init
CONFIG_NANO_ROS_INIT_DELAY_MS=2000
```

## Integration

Add to your Zephyr workspace's `west.yml`:

```yaml
manifest:
  projects:
    - name: nano-ros
      url: https://github.com/example/nano-ros
      revision: main
      path: modules/nano-ros
```

Then in your CMakeLists.txt:

```cmake
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(my_app)

target_sources(app PRIVATE src/main.c)
```

## API Reference

### Initialization

- `nano_ros_bsp_init(ctx)` - Initialize with Kconfig settings
- `nano_ros_bsp_init_with_locator(ctx, locator)` - Initialize with custom locator
- `nano_ros_bsp_shutdown(ctx)` - Cleanup and release resources
- `nano_ros_bsp_is_ready(ctx)` - Check if initialized

### Node

- `nano_ros_bsp_create_node(ctx, node, name)` - Create a node
- `nano_ros_bsp_create_node_with_domain(ctx, node, name, domain_id)` - Create with custom domain

### Publisher

- `nano_ros_bsp_create_publisher(node, pub, topic, type_name)` - Create publisher
- `nano_ros_bsp_publish(pub, data, len)` - Publish message
- `nano_ros_bsp_destroy_publisher(pub)` - Cleanup

### Subscriber

- `nano_ros_bsp_create_subscriber(node, sub, topic, type_name, callback, user_data)` - Create subscriber
- `nano_ros_bsp_destroy_subscriber(sub)` - Cleanup

### Spinning

- `nano_ros_bsp_spin_once(ctx, timeout)` - Process events once
- `nano_ros_bsp_spin(ctx)` - Spin forever

## License

MIT OR Apache-2.0
