The NuttX build script takes the board's Make.defs from the board defconfig
instead of defaulting to the QEMU ARM board, and refuses a Make.defs for a
different architecture than the defconfig's. A relative NuttX defconfig path
is now read from where you ran the build, and a missing one is reported by
name.
