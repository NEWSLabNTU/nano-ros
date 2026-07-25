# ws-remap-rust — launch/model remap + `~` private names (phase-305 W4)

A minimal product-shaped nano-ros workspace proving the **remap seam** end to
end: the Node pkg publishes on the PRIVATE name `~/out`, the system model
namespaces the node under `/island` and remaps `~/out` → `/remapped_out`, and
the topic that reaches the wire is the **remapped** one.

## The seam it shows

- `remap_talker_pkg` calls `create_publisher_for_topic::<Int32>("~/out")` —
  a source-level private name, never an absolute path in code.
- `demo_bringup/config/system_model.yaml` places the node at
  `/island/remap_talker` and carries

  ```yaml
  remaps:
    - from: "~/out"
      to: /remapped_out
  ```

- `nros::main!(model = "demo_bringup")` bakes the rules into
  `runtime.remaps` before the node's `register` call (phase-305 W3, issue
  0255); entity creation expands `~/out` against the node identity
  (`/island/remap_talker/out`) and resolves it through the rules — the wire
  name is `/remapped_out`. Without the remap the wire name would be the `~`
  expansion.

## Build & run

```sh
nros ws sync
cargo build -p native_entry
NROS_LOCATOR=tcp/127.0.0.1:7447 ./target/debug/native_entry
# in another shell: subscribe /remapped_out (NOT /island/remap_talker/out)
```

Exercised by `packages/testing/nros-tests/tests/workspace_features_e2e.rs`
(`native_rust_remap` cell): a sink on `/remapped_out` must receive; a sink on
the unremapped expansion must stay silent.
