BLOCKERS
1. **L86–87**: `cd examples/.../talker && cargo build --release` fails (exit 101) — example `Cargo.toml` has no empty `[workspace]` table, so cargo finds the parent superproject and rejects it. Reproduces from `just qemu build` too. (N2 from earlier batch-2 audit — ~80 examples need the empty `[workspace]` table.)
2. **L101**: `just qemu-baremetal zenohd` — recipe namespace doesn't exist. Correct: `just qemu zenohd`. A `just/qemu-baremetal.just` file exists in-tree but is not imported.
3. **L107**: `just qemu-baremetal talker` — same issue. Correct: `just qemu talker`.
4. **`just qemu zenohd` itself fails (exit 127)** — hardcodes `build/zenohd/zenohd` but `nros setup --rmw zenoh` installs zenohd to `~/.nros/sdk/zenohd/<v>/bin/zenohd` (with `~/.nros/bin/zenohd` shim). The recipe never consults the nros-CLI SDK store.

FRICTION
- `.cargo/config.toml` runner is bare `-kernel` — no `-nic socket,model=lan9118,…` flag, contradicting L137's claim.
- `generated/` codegen step is never invoked (Cargo's `[patch.crates-io.std_msgs] path = "generated/std_msgs"` requires that path populated, but no `nros generate-rust` step is documented).

CLARITY
- Configure section CLEAR (post-E.1 + E.12).
- Run section BROKEN by namespace + path issues above.

MISSING STEPS
- No `nros generate-rust` invocation before `cargo build` (the `generated/` dir is gitignored and not pre-populated).

NITs
- None.

WORKS
- `nros 0.3.7 setup qemu-arm-baremetal --rmw zenoh` (idempotent, exit 0).
- `nros.toml` doc block byte-accurate.
- GitHub source links resolve.

Acceptance bar (0 BLOCKERS): **NOT MET** (4 BLOCKERS — recipe namespace + build/zenohd path + workspace issue + codegen invocation).

LAST COMMAND: just qemu build
LAST EXIT CODE: 101
