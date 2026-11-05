# The Target Model

<!-- type: explanation · level: 2 -->

Every place an image can run is described the same way: a **launcher or a
flasher** (a host process, an emulator, or a tool that programs a board), a
**console** (where the image's output appears), and a **link** (how its
middleware reaches the rest of the system). `nros` reads these from the
board's descriptor, so the same commands work on every board.

- `nros run` launches an image on the host or in an emulator.
- `nros flash` programs a real board, and `nros monitor` attaches to its
  console.
- `nros bench` records which board on your machine is on which port.

*This page is coming soon.* Until then, see the
[`nros` CLI reference](../reference/cli.md).

## Design background

- [RFC-0118: target execution model](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0118-target-execution-model.md)
