An image's network is now resolved once, by the CLI, and reaches every build
as one value (`NROS_IMAGE_NET`, also written to `build/nros/net/<image>.toml`).
In `[image.<id>.net.<if>]`, `network` defaults to the board's (`qemu` on QEMU
boards); an interface on `qemu` with no address takes `10.0.2.15`; `mac` may
be stated, and is otherwise derived per image; `address = "dhcp"` is accepted
on a network with `dhcp = true` when the board's interface runs a DHCP client;
and an image may address several interfaces. A network's `router` is the
default locator of the images on it, below the image's own `locator` and above
`[system] locator`. Two images with the same address or MAC on one shared
network are refused, naming both. Board descriptors no longer state an image's
identity: `[board.net] ip` / `netmask` / `gateway` / `mac` are refused, and an
interface may carry a `mac_prefix`. The `NROS_ENTRY_IP_LAST` /
`NROS_APP_NET_IP_LAST` / `NROS_APP_NET_MAC_LAST` cmake overrides and
`nros board net --ip-last` / `--mac-last` are gone; state `host` / `mac` in the
image instead. An image that states no address now boots at `10.0.2.15` on
every QEMU board (FreeRTOS C/C++ images used `10.0.2.10`, esp32 `10.0.2.50`,
riscv64 `10.0.2.40`). `[image.*.net]` on a Zephyr board is refused: its
address is Kconfig's.
