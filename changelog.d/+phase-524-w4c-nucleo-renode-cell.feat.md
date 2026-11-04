The Nucleo-F767ZI's Renode twin is a test platform of its own,
`PlatformId::ZephyrNucleoF767zi`, with one Runtime cell: the C talker's serial
image (`[image.nucleo_serial]` of `examples/zephyr/c/talker`) boots in Renode
and a host listener hears it through the router's `serial/` locator
(`tests/zephyr_renode_twin.rs`). `nros locate tool renode` answers where Renode
is (`$NROS_RENODE`, then the store at the pinned `[tool.renode]`), and
`just zephyr setup` installs it. The board's registry row names the platform
and the `renode-nucleo` probe track; its cadence stays manual until a second
maintainer and the cell's first green lane run.
