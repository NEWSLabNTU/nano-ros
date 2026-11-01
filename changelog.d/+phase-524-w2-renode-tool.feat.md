`nros setup --tool renode` installs Renode 1.17.0, the emulator twin of the
Nucleo-F767ZI, from upstream's portable tarball (Linux x86_64 and arm64; macOS
has no portable archive upstream) and links `renode` into `$NROS_HOME/bin`. The
Zephyr 3.7 workspace now fetches `hal_stm32`, which the board needs.
