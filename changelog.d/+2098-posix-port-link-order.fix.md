A posix Rust binary no longer fails to link with `undefined reference to
nros_platform_clock_ns` when nothing above `nros-log` pulls the POSIX port.
`nros-platform-cffi` now anchors the port's `platform.o` ahead of the crates it
depends on. Before, `log-arena-probe` failed this way and stopped the Linux
fixture lane (issue 2098).
