Three names deprecated in `0.5.0-nros2` are **retired** (phase-484 W8c). Each
is now an error naming its replacement, never silently ignored:

- **`ZENOH_LOCATOR` / `ZENOH_MODE`** (runtime env) — use `NROS_LOCATOR` /
  `NROS_SESSION_MODE`. A process that still sets either refuses to start:
  Rust's `resolve_hosted` / `ExecutorConfig::from_env` panic and `nros::init`
  returns `InitError::RetiredEnvVar`; the C and C++ init entries return
  `RCL_RET_INVALID_ARGUMENT` / `NROS_CPP_RET_INVALID_ARGUMENT` and log the
  reason at error severity. Ignoring the variable would have been worse: the
  locator it carried would be dropped and the node would dial the default
  router, which reads as "no delivery". Presence alone refuses (an empty value
  too), including beside a canonical value. The native `just zenohd` recipe and
  the `scripts/debug/` helpers read `NROS_LOCATOR` now.
- **`nros::main!(board = X)`** — a `compile_error!`. Write `nros::main!()` and
  state the board in the leaf's `system.toml` (`[image.<id>] board`); a board
  crate outside the in-tree table states its own type in its manifest,
  `[package.metadata.nros.board] type = "::my_board::MyBoard"`.
- **`--features rmw-xrce`** on the three PX4 companion examples — the inert
  feature is gone (the xrce backend has been unconditional since phase-277),
  and `just px4` no longer passes it.
