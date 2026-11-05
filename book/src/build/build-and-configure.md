# Building and Configuring

`nros build` is the recommended way to build an image: it prepares
everything the build needs (generated messages, the resolved system, the
image's build settings) and then runs the host build tool, as `cargo build`
does. A project that stays in west, CMake, NuttX `make` or an IDE runs the
preparation step, `nros configure`, from that tool's own configure step
instead.

*This page is coming soon.*

## Design background

- [RFC-0120: one resolver for every build road](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0120-nros-configure-one-resolver.md)
