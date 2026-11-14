# Third-party code

<!-- type: reference · level: 2 -->

What an image compiles in besides nano-ros itself, under which license, and
where nano-ros carries patches. Use it to answer a reviewer's "what is in this
binary, and from whom". Which rows apply follows from three choices in the
image's `system.toml`: its platform (the board's), its RMW, and its locator.

## C and C++ sources

| Component | Compiled into | License | Upstream | Patches |
| --- | --- | --- | --- | --- |
| zenoh-pico | every image with `rmw = "zenoh"` | EPL-2.0 OR Apache-2.0 | [eclipse-zenoh/zenoh-pico](https://github.com/eclipse-zenoh/zenoh-pico) | [fork delta](forks/zenoh-pico.md) |
| Mbed TLS | a zenoh image built with TLS (the `link-tls` feature) | Apache-2.0 OR GPL-2.0-or-later | [Mbed-TLS/mbedtls](https://github.com/Mbed-TLS/mbedtls) | none |
| Micro XRCE-DDS Client, Micro-CDR | every image with `rmw = "xrce"` | Apache-2.0 | [eProsima](https://github.com/eProsima/Micro-XRCE-DDS-Client) | none |
| Eclipse Cyclone DDS | every image with `rmw = "cyclonedds"` | EPL-2.0 OR EDL-1.0 (BSD-3-Clause) | [eclipse-cyclonedds/cyclonedds](https://github.com/eclipse-cyclonedds/cyclonedds) | [fork delta](forks/cyclonedds.md) |
| FreeRTOS kernel | FreeRTOS boards | MIT | [FreeRTOS/FreeRTOS-Kernel](https://github.com/FreeRTOS/FreeRTOS-Kernel) | none |
| lwIP | FreeRTOS boards with a network | BSD-3-Clause | [lwip-tcpip/lwip](https://github.com/lwip-tcpip/lwip) | [fork delta](forks/lwip.md) |
| Eclipse ThreadX | ThreadX boards | MIT | [eclipse-threadx/threadx](https://github.com/eclipse-threadx/threadx) | [fork delta](forks/threadx.md) |
| NetX Duo | ThreadX boards with a network | MIT | [eclipse-threadx/netxduo](https://github.com/eclipse-threadx/netxduo) | [fork delta](forks/netxduo.md) |
| Apache NuttX | NuttX boards | Apache-2.0 | [apache/nuttx](https://github.com/apache/nuttx) | [fork delta](forks/nuttx.md) |
| Zephyr RTOS and its modules | Zephyr boards | Apache-2.0 (modules: see each module) | [zephyrproject-rtos/zephyr](https://github.com/zephyrproject-rtos/zephyr) | none; fetched by `west` at the version the SDK index pins |

The host-side tools `nros setup` installs (the cross-compilers, QEMU, the XRCE
agent) build and run images but are not linked into one. QEMU is a fork, and
its delta is listed under [Forks](forks/index.md) with the others.

## Rust crates

An image's Rust dependencies are an ordinary cargo graph. List them, with
their licenses, from the image's generated cargo root — the directory
`nros build` prints on its `entry →` line:

```text
cargo metadata --format-version 1 --manifest-path <entry-dir>/Cargo.toml \
  | python3 -c 'import json,sys; [print(p["name"], p["version"], p.get("license") or "-") for p in json.load(sys.stdin)["packages"]]' \
  | sort -u
```

That lists every crate cargo resolved, build-time ones included. None of
them is a fork: every crate comes from crates.io or from this repository.

## Forks

nano-ros builds some of these from a patch branch on its own fork rather than
from the upstream release. Each fork's page lists the commits it carries on
top of a pinned upstream ref, generated from the pin the release builds:
[Forks nano-ros carries](forks/index.md).
