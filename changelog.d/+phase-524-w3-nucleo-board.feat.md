New board package `nucleo-f767zi-zephyr` (ST Nucleo-F767ZI on Zephyr 3.7), with
Renode as its emulator twin: `nros setup nucleo-f767zi-zephyr` installs the
Zephyr SDK and Renode. A Zephyr leaf image now also gets its board package's
devicetree overlay (`boards/<board>.overlay`), not only its Kconfig, and a leaf
whose images name different boards resolves the board facts of the image it
builds.
