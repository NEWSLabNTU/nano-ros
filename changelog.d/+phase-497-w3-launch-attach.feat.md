A network's `attach` now decides how an emulated image joins it: `"slirp"`,
`{ bridge = "<dev>" }` or `{ tap = "<dev>" }` become QEMU's `-nic user`,
`-nic bridge,br=<dev>` and `-nic tap,ifname=<dev>`, with the image's MAC. A
network with no `attach` is physical only and refuses an emulated image at
launch. `nros ws qemu-nic <leaf> --model <m>` prints the `-nic` value for a
script that starts QEMU. An image's resolved network is now always at
`build/nros/net/<image>.toml`. The `mps2-an385` board's `docker` cargo feature
is removed: state the container's network as a `[network.*]` table and the
image's address on it.
