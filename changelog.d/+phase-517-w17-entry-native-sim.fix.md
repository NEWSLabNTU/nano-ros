`nros new entry --platform zephyr` writes `CONFIG_NET_DRIVERS=y` into its
native_sim board conf. Without it Zephyr dropped the offloaded-sockets symbol
and the build aborted on the Kconfig warning, so a scaffolded Rust entry did
not build on native_sim (phase-517 W17).
