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
nros sync examples/workspaces/sizing     # produces the sidecar
cargo build --manifest-path examples/workspaces/sizing/Cargo.toml
NROS_ENTRY_SPIN_MS=3000 ./target/debug/native_entry
```

Delete the sidecar and rebuild to see the pre-307 failure — the sizing falls
back to the model bound and the sixth `create_wall_timer` returns `Full`.
