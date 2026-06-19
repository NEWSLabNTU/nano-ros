# E2E-safety (CRC) showcase workspace

A nano-ros **differentiator** demo (phase-263 B1): wire-level message integrity —
a CRC + sequence number attached on publish and validated on receive — declared
once in `system.toml` and lowered to every language.

```
src/talker_pkg/        — Node pkg, publishes std_msgs/Int32 on /chatter.
src/safe_listener_pkg/ — Node pkg, SAFETY subscription on /chatter; reads the
                         per-message integrity status (CRC / gap / duplicate).
src/demo_bringup/      — Bringup: system.toml declares `features = ["safety"]`.
src/native_entry/      — Entry, boots talker + safe_listener on the native board.
```

## How safety is declared

One line in `demo_bringup/system.toml`:

```toml
[system]
features = ["safety"]
```

The capability registry (phase-261) lowers `features = ["safety"]` to:

- **Rust** — the `safety-e2e` features on the backend (`nros-rmw-zenoh`) + the
  umbrella (`nros`). The backend attaches the CRC on publish + validates on
  receive; `CallbackCtx::integrity()` exposes the result.
- **C/C++** — the `NANO_ROS_SAFETY_E2E` CMake option (phase-261 W5).

(This plain-cargo entry sets the `safety-e2e` cargo features explicitly in
`native_entry/Cargo.toml`; a `nros codegen-system` bake derives them from
`system.toml` automatically.)

## Build & run

```bash
source ./activate.sh
cd examples/workspaces/ws-safety-rust
nros setup native
nros ws sync
cargo run -p native_entry
```

Zenoh only — the CRC path lives in the zenoh backend.
