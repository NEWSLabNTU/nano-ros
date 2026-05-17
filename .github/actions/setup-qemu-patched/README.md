# `setup-qemu-patched` Composite Action

Phase 143.8 — load (or build + cache) the project-local patched
`qemu-system-arm` at `build/qemu/bin/qemu-system-arm`.

The test harness picks this binary up automatically via
`nros_tests::qemu::qemu_system_arm_path()`. No env var needed.

## Usage

```yaml
jobs:
  qemu-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: false   # this action inits qemu shallowly itself
      - uses: ./.github/actions/setup-qemu-patched
      - run: cargo nextest run -p nros-tests --test qemu_patched_binary
```

For workflows that already provision their own apt deps (custom
container image, etc.), opt out of the apt step:

```yaml
      - uses: ./.github/actions/setup-qemu-patched
        with:
          install-apt-deps: "false"
```

## Caching

Cache key composition:

```
qemu-patched-<os>-<arch>-<qemu-sha>-<patches-hash>
```

where:

- `<qemu-sha>` is the `third-party/qemu/qemu` SHA **recorded by the
  superproject** (not the working tree's checked-out tip — that
  may drift if a previous step ran `git submodule update` against a
  different ref).
- `<patches-hash>` is `sha256sum` over `find … | sort -z | xargs
  -0 sha256sum` of `third-party/qemu/patches/*` — both a submodule
  pin bump AND a patch series edit invalidate the cache
  automatically.

No `restore-keys` fallback: a partial / older cache for a
different submodule SHA or patch series would link mismatched
binaries against the `build/qemu/share/` data dir. Misses force a
full rebuild (~10 min on stock `ubuntu-latest`).

## Outputs

| Output      | Description                                                                |
|-------------|----------------------------------------------------------------------------|
| `qemu-bin`  | Absolute path to the installed `qemu-system-arm` binary.                   |
| `cache-hit` | `"true"` if the binary came from cache, `"false"` if rebuilt.              |
| `cache-key` | The composite cache key (qemu SHA + patches hash) — useful for debugging.  |

## Verification

The action's final step asserts:

- The binary exists and is executable.
- `--version` reports ≥ 7.2 (the `-netdev dgram` cutoff).
- `-netdev help` advertises `dgram`.

Any of these failing surfaces a `::error::` annotation and fails
the job — catches a stale cache or a too-old submodule pin loudly
rather than letting downstream test failures bury the root cause.

## Adding a new patch / bumping the pin

See `book/src/internals/qemu-patched-binary.md` — the
"Adding a new patch" and "Submodule pin bump" sections cover the
host-side workflow. After either change, the next CI run misses
the cache and rebuilds; no edits to this action are needed.
