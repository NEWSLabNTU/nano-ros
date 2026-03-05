# zpico-zephyr

Zenoh-pico platform support for Zephyr RTOS.

This library provides platform-level functions for running zenoh-pico on
Zephyr: network readiness, zenoh session lifecycle, and Zephyr logging.

For the full nano-ros API, use the root Zephyr module (`zephyr/module.yml`)
which provides the nros-c library (C) or Kconfig-to-Cargo bridging (Rust).

## API

```c
#include <zpico_zephyr.h>

// Wait for Zephyr network interface to come up
zpico_zephyr_wait_network(2000);

// Initialize and open zenoh session
zpico_zephyr_init_session("tcp/192.0.2.2:7447");

// ... use nros-c or nros Rust API ...

// Shut down
zpico_zephyr_shutdown();
```

## Integration

This crate is compiled automatically by the root nano-ros Zephyr module when
`CONFIG_NROS=y` is set in `prj.conf`. No manual integration is needed.

## License

MIT OR Apache-2.0
