BLOCKERS
- `nros setup threadx-linux --rmw zenoh` fails immediately: `Error: invalid SDK index nros-sdk-index.toml ... unknown field 'shallow'` (TOML line 286). Same failure for `nros setup qemu-riscv64-threadx --rmw zenoh`. Installed `nros` at `~/.nros/bin/nros` rejects the in-tree index's `shallow`/`recursive` keys at the end of the `[source.px4]` block. Setup is step 1; every later prerequisite (zenohd, ThreadX/NetX sources, `tap-tx0` veth) is unprovisioned — `~/.nros/sdk/threadx/{kernel,netxduo}` is empty, `build/zenohd/zenohd` absent, `zenohd` not on PATH, `tap-tx0` does not exist.

FRICTION
- `~/.nros/bin` is not on PATH by default; tutorial does `export PATH=` first but `source ./setup.bash` (which would have done it) is wedged in the same code block as the failing `nros setup` invocation.

CLARITY
- Setup: VAGUE — no verification step after `nros setup`.
- Project layout: CLEAR. Configure: WRONG (drifts below). Build/Run: CLEAR shape, no preflight.

MISSING STEPS
- No verification `zenohd` is running before `cargo run`.
- threadx-riscv64 Run block shows `qemu-system-riscv64 -kernel ./build/talker.elf` but never describes how that elf is produced by `just threadx_riscv64 build-fixtures` or where it lands.
- ROS 2 verification block needs stock ROS 2 install; presented as optional, no pointer.

WORKS
- File is internally consistent. `just threadx_linux {build-fixtures,test}` and `just threadx_riscv64 build-fixtures` recipes exist. Example trees at documented paths.

DRIFTS
- Config filename: doc says `config.toml`; actual is `nros.toml`.
- Config schema: doc uses `[network]/[platform]/[zenoh]`; actual schema is `[node]` + `[[transport]]` with `kind = "ethernet"`, CIDR `ip` (`192.0.3.10/24`), `interface`, `locator` all nested.
- riscv64 fixture drift: doc `ip=10.0.2.10 mac=02:…`; actual `ip=10.0.2.40/24 mac=52:54:00:12:34:56`.
- Expected stdout banner: doc shows `nros ThreadX-Linux Talker` / `Published: 1, 2, …`; actual prints `Declaring publisher on /chatter (std_msgs/Int32)` / `Publisher declared` / `Publishing messages…` / `Published: 0, 1, …` (counter starts at 0).
- GitHub board crate link (NIT): doc points to `packages/boards/nros-board-riscv64-qemu/`; actual crate is `nros-board-threadx-qemu-riscv64`.

LAST COMMAND: cd examples/threadx-linux/rust/talker && cargo build --release
LAST EXIT CODE: 101
