BLOCKERS
- Setup: `nros setup qemu-arm-freertos --rmw zenoh` fails with `invalid SDK index nros-sdk-index.toml ... unknown field 'shallow'`. Installed CLI is 0.3.1; `nros-sdk-index.toml:286` requires `nros ≥ 0.3.7` (matching `scripts/install-nros.sh` pin). Tutorial silently relies on a fresh install of the installer pin; user has no way past this without out-of-band knowledge.
- Build (Rust): `cd examples/qemu-arm-freertos/rust/talker && cargo build --release` fails — example's `Cargo.toml` has no empty `[workspace]`, so any nested checkout (here, the audit worktree under nano-ros) breaks "standalone copy-out" claim.
- Build (C single-example): `cmake --build build --parallel` succeeds for FreeRTOS+lwIP+LAN9118 but fails in corrosion stage because `nros-tests` transitively requires `third-party/px4/px4-rs/tests/sitl/` (missing dir → cargo dep resolve fails).
- `just freertos build-fixtures` — same workspace conflict as Rust single-example.

FRICTION
- ARM toolchain at `~/.nros/sdk/arm-none-eabi-gcc/13.2-nros1/bin/` not auto-on-PATH.

CLARITY — VAGUE
- Configure section: every key wrong (see MISSING).
- Run section never mentions the `.cargo/config.toml` QEMU runner backing `cargo run`.

MISSING STEPS
- Project layout shows `config.toml`; actual file is `nros.toml`. C/ and cpp/talker have no `package.xml` (only Rust does). No `generated/` dir.
- Configure block documents `[network]`/`[zenoh]`/`[scheduling]` schema with `ip`/`mac`/`gateway`/`netmask`/`locator`/`domain_id`/`app_priority`. Actual file is `[node]` / `[[transport]] kind="ethernet"` / `[node.rt]` — every key name wrong. Documented IP `10.0.2.21`; file has `10.0.2.20/24`.
- No `direnv allow` step (CLAUDE.md says required).

WORKS
- `nros 0.3.1` binary runs.
- CMake configure of single C talker succeeds (~149 s), auto-provisioning FreeRTOS/lwip/zenoh-pico/mbedtls via its own `nros setup --source` driver — that internal path tolerates `shallow=true` unlike top-level `nros setup`.
- `cmake/toolchain/arm-freertos-armcm3.cmake` exists exactly as tutorial states.
- Binary name `freertos_c_talker` matches tutorial.
- `just freertos {build, build-fixtures, test}` recipes exist as advertised.

LAST COMMAND: timeout 60 just freertos build
LAST EXIT CODE: 101
