# `sample.yaml` Schema

**Status:** Draft (Phase 114.A.1)
**Audience:** Example authors, `nros run` users, test-harness maintainers.
**Related:** `docs/roadmap/phase-114-sample-metadata-and-msg-bundles.md`,
`docs/roadmap/phase-111-ux-cli-and-release-channels.md`.

---

## Scope

`sample.yaml` is the **single source of truth** for per-example metadata in
nano-ros: which boards an example runs on, which tags it carries, what
stdout pattern marks success, what extra build args it needs. The format
is a **strict subset of Zephyr Twister's `sample.yaml` / `testcase.yaml`
schema** — every key listed below is parsed by Twister with identical
semantics, so Zephyr-targeted nano-ros examples plug into the upstream
Twister runner unchanged. For non-Zephyr platforms (FreeRTOS, NuttX,
ThreadX, POSIX, bare-metal), the same file is consumed by
`nros run` (Phase 111), the `sample-walker` catalog tool
(Phase 114.A.2), and the `just test-all` discovery sweep
(Phase 114.A.5). Keys outside this subset are silently ignored by
nano-ros tooling; do not rely on them.

---

## Discovery rule

`sample-walker` (and therefore `nros run`, `just test-all`, the
auto-generated `book/src/examples.md` index) discovers **any
`sample.yaml` file** living under one of the following roots:

- `examples/` — public, copy-out user examples.
- `packages/testing/nros-bench/<name>/` — performance / fairness /
  stress / large-msg benches.
- `packages/testing/nros-smoke/<name>/` — board / driver bringup
  binaries (no nros API).
- `packages/testing/nros-tests/bins/<name>/` — fixture binaries that
  integration tests spawn.

Walk is recursive; a directory containing both a `sample.yaml` **and**
a `Cargo.toml` (or `CMakeLists.txt`, or `west.yml`, or `prj.conf`) is
treated as a buildable unit. A `sample.yaml` without a sibling
buildable file is a hard error — the walker emits the offending path
and exits non-zero. There is no other discovery mechanism: if it lives
outside those four roots, nano-ros tooling will not see it.

The walker emits a JSON catalog (`target/sample-catalog.json`) keyed by
the canonical test name (`tests.<name>` from the file), with every
field flattened from `common` into each test scenario for downstream
consumers.

---

## Schema

### Top-level keys

| Key      | Type   | Required | Purpose                                                                      | Default |
|----------|--------|----------|------------------------------------------------------------------------------|---------|
| `sample` | object | yes      | Example identity (name + human description).                                 | —       |
| `common` | object | no       | Per-test keys applied to every scenario in `tests` (merged, not overridden). | `{}`    |
| `tests`  | map    | yes      | Map of test-scenario name → per-test config. At least one entry required.    | —       |

`sample` object fields:

| Key           | Type   | Required | Purpose                                                                                | Default |
|---------------|--------|----------|----------------------------------------------------------------------------------------|---------|
| `name`        | string | yes      | Stable identifier. Convention: `nros_<platform>_<lang>_<rmw>_<example>`.               | —       |
| `description` | string | yes      | One-line human summary, used in `book/src/examples.md` and `nros run --list`.          | —       |

`tests` map keys are scenario identifiers. Convention:
`sample.nros.<platform>.<lang>.<rmw>.<example>[.<variant>]`. Dot-segmented
so Twister's `--test` filter (and the future `nros run --test`) selects on
prefix.

### Per-test keys (under `tests.<name>` or `common`)

If a key is present under both `common` and `tests.<name>`, the
per-test value **replaces** the common value (lists are not merged —
this matches Twister behaviour).

| Key                            | Type            | Required | What nano-ros uses it for                                                                                                                                                                            | Default                |
|--------------------------------|-----------------|----------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------------|
| `tags`                         | string \| list  | yes      | Whitespace-separated tags (string form) or list. Drives `just test-all` group selection and book filtering. Conventional tags: `nros`, `nros-c`, `nros-cpp`, `rmw-zenoh`, `rmw-xrce`, `rmw-dds`, `rmw-cyclonedds`, `platform-{posix,freertos,nuttx,threadx,zephyr,bare-metal}`, `slow`, `network`, `qemu`. | —                      |
| `harness`                      | string          | yes      | Which runner consumes the binary's output. nano-ros supports: `console` (stdout-regex), `pytest` (delegated to Twister on Zephyr only), `script` (run an arbitrary shell script under `tests_scripts/`). All others (`ztest`, `gtest`, `bsim`, `robot`, `power`, `shell`, `display_capture`, `ctest`) are accepted but treated as `console` by the non-Twister path. | —                      |
| `harness_config`               | object          | no       | Harness-specific knobs. See subsection below.                                                                                                                                                        | `{}`                   |
| `platform_allow`               | list[string]    | no       | Whitelist of boards (matches `nros board list` identifiers, e.g. `mps2/an385`, `native_sim`, `qemu_riscv32`). If empty/absent the example is considered platform-agnostic.                            | `[]`                   |
| `platform_exclude`             | list[string]    | no       | Blacklist of boards. Applied after `platform_allow`.                                                                                                                                                 | `[]`                   |
| `integration_platforms`        | list[string]    | no       | Subset of `platform_allow` that CI exercises in the default (non-`--full`) pass. Other allowed boards build on demand only. Mirrors Twister's `--integration` flag.                                  | `[]`                   |
| `extra_args`                   | list[string]    | no       | Extra Cargo / CMake args. nano-ros also accepts `KEY=VALUE` shell-style strings (e.g. `NROS_RMW=zenoh`, `NROS_LANG=c`); these set env vars during build/run. Twister namespacing prefixes (`arch:`, `platform:`, `simulation:`) are honored by the Zephyr path only. | `[]`                   |
| `extra_configs`                | list[string]    | no       | Kconfig overrides (`CONFIG_FOO=y`) merged into `prj.conf` on Zephyr. Ignored on non-Zephyr platforms.                                                                                                | `[]`                   |
| `timeout`                      | integer (s)     | no       | Wall-clock budget for `nros run` / `just test-all` per scenario.                                                                                                                                     | `60`                   |
| `skip`                         | bool            | no       | Skip the scenario unconditionally. `nros run` reports `[SKIPPED]` and exits 0.                                                                                                                       | `false`                |
| `slow`                         | bool            | no       | Only run when `--enable-slow` is passed. CI sets this on the nightly long-soak job.                                                                                                                  | `false`                |
| `fixture`                      | string \| list  | no       | External-resource dependency. nano-ros recognised values: `zenohd`, `xrce-agent`, `cyclonedds-router`, `tap-bridge`, `ros2-talker`, `ros2-listener`. The runner refuses to start unless the fixture is reachable. | `[]`                   |
| `depends_on`                   | list[string]    | no       | Board-feature requirements (e.g. `netif:eth`, `i2c`). Used by the Zephyr Twister path. nano-ros tooling treats unknown values as a no-op.                                                            | `[]`                   |
| `min_ram`                      | integer (KB)    | no       | Reserved for future board-descriptor filtering (Phase UX-42). Currently parsed and round-tripped into the catalog but does not gate execution.                                                       | unset                  |
| `min_flash`                    | integer (KB)    | no       | Same as `min_ram` — round-tripped, not enforced yet.                                                                                                                                                 | unset                  |
| `filter`                       | string          | no       | Boolean expression over `CONFIG_*` / `ARCH` / env-var symbols. Honored by the Zephyr Twister path verbatim. The non-Twister path evaluates a strict subset: `CONFIG_<NAME>` lookups against `config.toml` and `env("NAME")` lookups against process env. Unknown symbols → expression evaluates to false (scenario skipped). | unset                  |
| `arch_allow` / `arch_exclude`  | list[string]    | no       | Architecture filtering. Honored as written on Zephyr; non-Zephyr platforms reduce architecture to `posix`, `arm`, `aarch64`, `riscv32`, `riscv64`, `xtensa`.                                         | `[]`                   |

### `harness_config` subkeys

Only the `console` harness has nano-ros-defined semantics. The other
shapes (`pytest`, `script`, …) are passed through to whichever runner
consumes the file; on the non-Twister path they are ignored.

| Key       | Type            | Required (when `harness: console`) | Purpose                                                                                                                                                                                                                                | Default      |
|-----------|-----------------|------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------|
| `type`    | enum            | yes                                | `one_line` (any single output line must match) or `multi_line` (every regex must match at least one line). Mirrors Twister.                                                                                                            | —            |
| `regex`   | list[string]    | yes                                | List of patterns. Anchored implicitly with `re.search`; use `^…$` for full-line matches. Patterns run against stdout **and** stderr.                                                                                                   | —            |
| `ordered` | bool            | no                                 | When `multi_line`, require patterns to match in declaration order. Ignored for `one_line`.                                                                                                                                              | `false`      |
| `record`  | object          | no                                 | `{ "regex": "...", "as": "csv" \| "json" }` — extract named-group captures into `target/sample-records/<test>.csv`/`.json`. Used by perf benches to emit machine-readable measurements.                                                | unset        |

For the `script` harness, the single sub-key
`tests_scripts: list[string]` (defaulting to `tests_scripts/`) names
shell scripts run **after** the binary launches. Exit status 0 → pass.

---

## Worked examples

### Example 1 — POSIX native talker

`examples/native/rust/talker/sample.yaml`:

```yaml
sample:
  name: nros_native_rust_zenoh_talker
  description: Publishes std_msgs/Int32 on /chatter once a second.

common:
  tags: nros rmw-zenoh platform-posix network
  harness: console
  harness_config:
    type: one_line
    regex:
      - "Published: 5"
  fixture: zenohd
  timeout: 30

tests:
  sample.nros.native.rust.zenoh.talker:
    platform_allow:
      - native
    integration_platforms:
      - native
    extra_args:
      - NROS_RMW=zenoh
```

`nros run examples/native/rust/talker/` reads this file,
launches `zenohd` (because of `fixture: zenohd`), runs `cargo run`
on the sibling crate, and waits up to 30 seconds for the literal
string `Published: 5` to appear in stdout.

### Example 2 — Zephyr `talker` across multiple boards

`examples/zephyr/cpp/zenoh/talker/sample.yaml`:

```yaml
sample:
  name: nros_zephyr_cpp_zenoh_talker
  description: rclcpp-shaped talker on Zephyr with zenoh-pico transport.

common:
  tags: nros nros-cpp rmw-zenoh platform-zephyr
  harness: console
  harness_config:
    type: multi_line
    ordered: true
    regex:
      - "nros: zenoh session opened"
      - "Published: 1"
      - "Published: 5"
  fixture: zenohd
  timeout: 60
  extra_configs:
    - CONFIG_NROS_CPP_API=y
    - CONFIG_NET_LOG=y

tests:
  sample.nros.zephyr.cpp.zenoh.talker.native_sim:
    platform_allow:
      - native_sim
      - native_sim/native/64
    integration_platforms:
      - native_sim

  sample.nros.zephyr.cpp.zenoh.talker.qemu_cortex_m3:
    platform_allow:
      - qemu_cortex_m3
    integration_platforms:
      - qemu_cortex_m3
    extra_args:
      - "platform:qemu_cortex_m3:CONFIG_HEAP_MEM_POOL_SIZE=65536"

  sample.nros.zephyr.cpp.zenoh.talker.frdm_k64f:
    platform_allow:
      - frdm_k64f
    # Hardware-only: not in integration_platforms.
    slow: true
```

Twister consumes this file directly via
`west twister -T examples/zephyr/cpp/zenoh/talker/`. The non-Twister
nano-ros path also runs the `native_sim` scenario through
`nros run --platform native_sim`.

---

## What nano-ros explicitly does NOT support

These keys may appear in upstream Twister files. nano-ros tooling
**parses them without erroring** (so files round-trip), but assigns
them no meaning. Do not rely on them outside the Zephyr Twister path.

- **`build_only: true`** — nano-ros has no "build but don't execute"
  concept at the catalog level; if an example shouldn't run, gate
  it with `skip: true` plus a comment. CI build-only sweeps are
  expressed as `just <plat> build-examples`, not per-sample.
- **`sysbuild: true`** — nano-ros examples are single-image. Multi-image
  scenarios are out-of-scope until a concrete need arises.
- **`required_applications`** / **`required_devices`** — nano-ros
  multi-process tests are wired through `nros_tests` fixtures
  (pub/sub pairs, ROS 2 interop), not catalog cross-references. If
  you need a second binary, write a Rust integration test that
  spawns it.
- **`required_snippets`** — Zephyr-snippet-specific; nano-ros has no
  equivalent.
- **`platform_key`** — Twister deduplicates "run this once per
  arch/simulation tuple". nano-ros runs every entry in
  `integration_platforms` exactly once; if you want to run on three
  boards, list three boards.
- **`ztest_suite_repeat`**, **`ztest_test_repeat`**, **`ztest_test_shuffle`**
  — ztest-specific; nano-ros isn't a ztest consumer.
- **`extra_sections`** — Twister size-report knob; nano-ros uses
  `cargo bloat` / `arm-none-eabi-size` directly.
- **`vendor_allow`**, **`vendor_exclude`** — board metadata isn't
  modelled with a vendor field yet. Use `platform_allow` /
  `platform_exclude` instead.
- **`integration_toolchains`** — nano-ros pins one toolchain per
  platform; toolchain matrix expansion is not in scope.
- **`expect_reboot`** — nano-ros examples are not expected to reboot;
  if yours does, lift it into a smoke binary under
  `packages/testing/nros-smoke/`.
- **`harness: ztest|gtest|bsim|robot|power|shell|display_capture|ctest`**
  — accepted on input, treated as `console`. Use `console` explicitly
  unless you really are feeding Twister on Zephyr and want it to dispatch
  to one of those harnesses.
- **`harness_config.record`** beyond `csv|json`, custom `pytest_dut_scope`
  values, BabbleSim-specific keys — Zephyr-Twister-only.

The most pointed omission, called out separately so contributors do
not miss it: **`build_only` is not honored**. nano-ros draws a sharp
line between "is this example shippable and runnable" (the catalog's
job) and "does this combination compile" (the per-platform build-tier
recipes' job). Mixing the two has caused upstream Zephyr CI to ship
samples that build forever and have never actually executed; we do not
want to import that failure mode.

---

## Validation

CI (Phase 114.A.5) runs `sample-walker --check` against every
discovered file. The walker enforces:

1. Required top-level keys (`sample.name`, `sample.description`,
   `tests` non-empty).
2. Every `tests.<name>` has at least one of (`tags`, common-inherited
   `tags`) and a `harness` value.
3. When `harness: console`, `harness_config.type` and
   `harness_config.regex` are present and non-empty.
4. Every `platform_allow` / `integration_platforms` /
   `platform_exclude` entry exists in the `nros board list` output
   **or** the Zephyr board database (the union, to allow Zephyr-only
   identifiers).
5. Every `fixture` value belongs to the recognised set
   (`zenohd`, `xrce-agent`, `cyclonedds-router`, `tap-bridge`,
   `ros2-talker`, `ros2-listener`); unknown fixtures are an error
   (typo guard).
6. `tests.<name>` keys are dot-segmented and unique across the
   catalog. Collisions across files are a hard error.

A schema file (`docs/reference/sample-yaml.schema.json`, generated
alongside the walker in 114.A.2) is published for editor integration.
