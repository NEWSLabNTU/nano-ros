# cmake/board/nano-ros-board-linux-aarch64-musl.cmake
#
# Issue 2852 (phase-589 W5) — the musl Linux cross board on the CMake road.
#
# The board builds Rust images (its descriptor's `cargo_config` carries the
# triple, zig as the C compiler and the static link). The C and C++ road is
# phase-589 W8: a node linking a vendor's shared libraries, built against the
# board's sysroot. Until then this road refuses, because without a cross
# toolchain here it would build a HOST program and call it this board's.
message(FATAL_ERROR
    "nano-ros: board `linux-aarch64-musl` builds Rust images only for now. "
    "C and C++ nodes on it are not supported yet: this configure would build "
    "a program for this machine rather than for the board. Write the image's "
    "nodes in Rust, or build C/C++ for the `linux` board.")
