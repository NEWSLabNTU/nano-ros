`nros new <name> --platform freertos|nuttx|threadx|zephyr --lang rust` still
writes no single package, and now prints the commands that start the
workspace instead: `nros new <name> --workspace --lang rust`, then
`nros new --image <id> --board <board>` with the platform's boards from the
catalog (one line per board for ThreadX), or `nros new entry` on Zephyr. It
used to name an in-tree example directory (phase-517 W13).
