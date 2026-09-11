# sizing — the executor-sizing showcase (issue 0257)

A workspace whose node the SystemModel cannot count.

`burst_pkg::BurstTalker` registers **six timers** and no subscription. Launch
wiring has no timer entity, so the model sees zero callback entities for this
node; the runtime needs six callback slots. The executor's table defaults to
four.

Before phase-307 the entry compiled cleanly and died at boot on the fifth
timer with `code=-6 Full`. Now `nros sync` records the six timers in
`src/burst_pkg/metadata/burst_talker.json`, `nros::main!` reads that sidecar,
and the entry opens the executor at the derived size.

```sh
source ./activate.sh
cd examples/workspaces/sizing
nros sync            # produces the sidecar, the generated entry and its settings
nros build native
NROS_ENTRY_SPIN_MS=3000 ./build/posix/native_entry/target/debug/native_entry
```

There is no `Cargo.toml` at the workspace root to build — the cargo root is the
entry `nros build` generates under `build/` (RFC-0098 D9).

Delete the sidecar and rebuild to see the pre-307 failure — the sizing falls
back to the model bound and the sixth `create_wall_timer` returns `Full`.
