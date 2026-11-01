# @NAME@

One package, one node, one board. Two things in it are yours to edit:

- `src/` holds the node: a @TEMPLATE@ (@SUMMARY@).
- `system.toml` says what the package runs on:

  ```toml
  [image.@IMAGE@]
  board = "@BOARD@"
  ```

  To run the same node on another board, change that one line.

Everything else here (`package.xml` and the build manifest beside it) only
names the package and its dependencies. The build settings for the board are
generated under `build/` each time you build, so the package carries no
`.cargo/` directory, no workspace root and no launch file.

## Build

From this directory:

```bash
@STEPS@
```

The build's last lines name what it produced and how to start it: an
`artifact:` path and a `run:` command for this board.

## A second node

When you want a second node, grow this package into a workspace. The node
moves over unchanged, and this `system.toml` moves beside it, still declaring
what the workspace runs on:

```bash
cd ..
nros new my_ws --workspace --from @NAME@
cd my_ws
nros new node second_node
```
