# Installation

This chapter covers setting up your development environment for nano-ros.
Choose the path that matches your language:

- **Rust users** — install two tools, no git clone needed.
- **C users** — build the nros C library from source and install to a prefix.

Both paths need a zenohd router for the Zenoh backend.

## zenohd Router

Download a prebuilt zenohd 1.6.2 binary from the
[zenoh releases](https://github.com/eclipse-zenoh/zenoh/releases/tag/1.6.2)
page. Extract it and place it somewhere on your `PATH`.

Verify it works:

```bash
zenohd --version
# zenohd 1.6.2
```

## Rust

### Prerequisites

- [Rust](https://rustup.rs/) nightly toolchain (edition 2024)
- A C compiler (gcc or clang) — needed to build zenoh-pico

### Install the Code Generator

`cargo-nano-ros` generates Rust bindings for ROS 2 message types. Install
it once:

```bash
cargo install --git https://github.com/jerry73204/nano-ros cargo-nano-ros
```

This fetches and builds the tool directly from the repository — no manual
git clone required.

Verify:

```bash
cargo nano-ros --version
```

That's it. You can now create nano-ros Rust projects. Continue to
[First App in Rust](first-app-rust.md).

## C

The C API is distributed as a static library with headers and CMake config.
You build it from source once and install to a prefix that your projects
reference via `CMAKE_PREFIX_PATH`.

### Prerequisites

- [Rust](https://rustup.rs/) nightly toolchain (needed to compile the
  Rust core that the C library wraps)
- CMake 3.22+
- A C compiler (gcc or clang)

### Build and Install

```bash
git clone https://github.com/jerry73204/nano-ros.git
cd nano-ros

# Build and install the zenoh variant
cmake -S . -B build -DNANO_ROS_RMW=zenoh -DCMAKE_BUILD_TYPE=Release
cmake --build build
cmake --install build --prefix ~/.local/nano-ros

# (Optional) Also install the XRCE-DDS variant to the same prefix
cmake -S . -B build-xrce -DNANO_ROS_RMW=xrce -DCMAKE_BUILD_TYPE=Release
cmake --build build-xrce
cmake --install build-xrce --prefix ~/.local/nano-ros
```

This installs to `~/.local/nano-ros/`:

```
~/.local/nano-ros/
├── bin/nros-codegen          # C code generator
├── include/nros/             # C headers
├── lib/
│   ├── libnros_c_zenoh.a     # Static library (zenoh)
│   ├── libnros_c_xrce.a      # Static library (XRCE, if installed)
│   └── cmake/NanoRos/        # CMake config-mode package
└── share/nano-ros/interfaces/ # Bundled .msg/.srv/.action files
```

You can use any prefix (`/usr/local`, a project-local directory, etc.).
Your C projects will reference it via `-DCMAKE_PREFIX_PATH=~/.local/nano-ros`.

Continue to [First App in C](first-app-c.md).

## Contributor Setup

If you want to build the full workspace, run all tests, or work on
nano-ros itself:

```bash
git clone https://github.com/jerry73204/nano-ros.git
cd nano-ros
just setup
```

`just setup` installs:

- Rust nightly toolchain and embedded targets (`thumbv7m-none-eabi`,
  `riscv32imc-unknown-none-elf`, etc.)
- Cargo tools: `cargo-nextest`, `cargo-nano-ros`, `cargo-binutils`
- System dependencies check (cmake, pkg-config, etc.)
- FreeRTOS kernel + lwIP sources (to `external/`)
- NuttX RTOS + apps (to `external/`)

> **Note:** `just setup` does not run `sudo`. If system packages are
> missing, it will tell you what to install.

Build everything:

```bash
just build
```

Run tests:

```bash
just test-unit     # Unit tests (fast)
just test          # Unit + Miri + QEMU
```

### Building zenohd from Source

The repository includes zenohd 1.6.2 as a git submodule. To build it
instead of using a prebuilt binary:

```bash
just build-zenohd
# Binary at: build/zenohd/zenohd
```

### Docker Environment

For a containerized environment (or QEMU 7.2+ for TAP networking):

```bash
just docker-build      # Build the nano-ros-qemu image
just docker-shell      # Interactive shell with all tools
just docker-test-qemu  # Run QEMU tests in container
```

## Next Steps

- [First App in Rust](first-app-rust.md) — build and run a Rust publisher
- [First App in C](first-app-c.md) — build and run a C publisher
