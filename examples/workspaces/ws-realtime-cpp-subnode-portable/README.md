# ws-realtime-cpp-subnode-portable — tier names are deployment-owned

The portability proof for [`ws-realtime-cpp-subnode`](../ws-realtime-cpp-subnode/):
the **identical** `SubNode` component (same groups `ctrl`/`telem`, same
topics) runs under a bringup whose tiers are named **`fast`/`bulk`** instead
of `high`/`low`. The package carries only group IDs — tier names belong to the
deployment.

## What it shows

- `src/subnode_pkg/` — functionally identical node logic to the `-subnode` variant.
- `src/deploy_bringup/` (note: not `demo_bringup`) —
  `group_tiers = { ctrl = "fast", telem = "bulk" }`, with `[tiers.fast]`
  (10 ms, posix prio 80) and `[tiers.bulk]` (100 ms, prio 10) in
  `system.toml`; the entry's `main.cpp` references
  `"deploy_bringup:system.launch.xml"`.

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup deploy_bringup
cmake -S . -B build && cmake --build build
zenohd --listen tcp/127.0.0.1:7447 &
./build/src/native_entry/native_entry
```

(Fixture id `workspace-cpp-native-realtime-subnode-portable`; e2e
`realtime_subnode_cpp_portable_e2e`.)

## Expected output

Identical to the `-subnode` variant — that is the point:

```
[subnode/ctrl] tick=N     # ~10 per one
[subnode/telem] tick=N
```
