# nano-ros-bsp-qemu

Board Support Package for running nano-ros on QEMU MPS2-AN385.

This crate provides a simplified API that abstracts away all hardware and network stack details. Users only need to focus on ROS concepts (publishers, subscribers, topics).

## Features

- **Zero boilerplate**: Single `run_node()` entry point handles all initialization
- **Sensible defaults**: Works out-of-box with QEMU and standard TAP networking
- **Pure ROS API**: No smoltcp, zenoh-pico, or hardware details exposed
- **Configurable**: Override defaults with builder pattern
- **~55% code reduction**: Examples reduced from 200+ lines to ~90 lines

## Usage

```rust
#![no_std]
#![no_main]

use nano_ros_bsp_qemu::prelude::*;
use panic_semihosting as _;

#[entry]
fn main() -> ! {
    run_node(Config::default(), |node| {
        let publisher = node.create_publisher(b"demo/topic\0")?;

        for _ in 0..10 {
            node.spin_once(10);
            publisher.publish(b"Hello from QEMU!")?;
        }

        Ok(())
    })
}
```

## Network Configuration

### Default (TAP networking)

Direct connection to host via TAP interface:

- IP: 192.0.3.10/24
- Gateway: 192.0.3.1
- Zenoh router: tcp/192.0.3.1:7447

### Docker mode

Enable the `docker` feature for container networking:

- IP: 192.168.100.10/24
- Gateway: 192.168.100.1
- Zenoh router: tcp/172.20.0.2:7447

### Custom configuration

```rust
let config = Config::default()
    .with_ip([10, 0, 0, 100])
    .with_gateway([10, 0, 0, 1])
    .with_zenoh_locator(b"tcp/10.0.0.1:7447\0");

run_node(config, |node| {
    // ...
    Ok(())
});
```

## Running with QEMU

```bash
# Start zenoh router
zenohd --listen tcp/192.0.3.1:7447

# Run the example (from example directory)
qemu-system-arm \
    -machine mps2-an385 \
    -cpu cortex-m3 \
    -nographic \
    -semihosting-config enable=on,target=native \
    -netdev tap,id=net0,ifname=tap0,script=no,downscript=no \
    -net nic,netdev=net0,model=lan9118 \
    -kernel target/thumbv7m-none-eabi/release/my-example
```

## License

MIT OR Apache-2.0
