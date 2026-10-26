---
id: 2068
title: "`nros setup` for a NuttX board does not ask for the host tools its kernel build needs"
status: resolved
type: bug
area: [nuttx, setup, installed-sdk]
severity: medium
found: 2026-10-26
resolved: 2026-10-27
related: [phase-517, issue-2064, RFC-0062]
---

## What

`nros setup qemu-armv7a-nuttx` (and `rv-virt-nuttx`) provisions the cross
toolchain, QEMU and the NuttX kernel/apps/libc sources, but not two host tools
the NuttX kernel build then needs:

- Python's `venv` module — `scripts/nuttx/build-nuttx.sh` provisions
  `kconfiglib` into a venv when `kconfig-frontends` is absent;
- `unzip` — NuttX's own build downloads and unzips `dtc` and `libmetal`.

The index already declares both (`[prereq.python3-venv]`, `[prereq.unzip]`,
and `[prereq.kconfig-frontends]` as the alternative), but nothing ties them to
a NuttX board: a `[tool.*]` can say `system = [..]` and the setup plan asks for
those keys before the download (that is how `[tool.zephyr-sdk]` asks for
`wget`), while a `[source.*]` (`SourcePackage`) and a `[board.*]`
(`BoardEntry`) have no such field.

Found replaying `integration-nuttx.md` on the installed track (phase-517 W10).
The chapter now names both as a step, which is the workaround, not the fix.

## Fix direction

Give `[source.*]` (or `[board.*]`) the same `system = [..]` list a tool has,
read by the same plan code, and declare `python3-venv` + `unzip` on
`nuttx-kernel`. Then drop the chapter's manual step.

## Acceptance

On a host without `python3-venv` and `unzip`, `nros setup qemu-armv7a-nuttx
--dry-run` lists both among the system packages it would install.

## Resolution

`[source.*]` (`SourcePackage`) now carries the same `system = [..]` list a
`[tool.*]` has, validated against `[prereq.*]` at load by the same rule, and
read by the same plan code: `SessionPlan::resolve` folds a source step's keys
into the one system-package ask. A source contributes whether or not the run
fetches it — the keys are what the BUILD reading the tree needs, which a
populated checkout does not satisfy — and a present key asks nothing, so a
repeat stays quiet.

`[source.nuttx-kernel]` declares `system = ["python3-venv", "unzip"]`.
`[prereq.python3-venv]` gained the probe it lacked (`python_import =
"ensurepip"` — Debian/Ubuntu split `ensurepip` into `python3.X-venv`, so
`import venv` succeeds without it; with no probe the key read `Unknown` and was
never asked). The `kconfig-frontends` alternative is NOT expressed: the prereq
model's `providers` order ways to satisfy ONE key, not a choice between keys, so
the venv is asked for unconditionally — the route that works on every host.

`integration-nuttx.md` no longer explains the two tools by hand; it says
`nros setup` names them and prints the install command. The `probe=35` block
stays, because the board path never runs `sudo` (`--sudo` with a board is
refused) and the probe's clean container must still install them.

## Evidence

- `shipped_index_nuttx_setup_asks_for_the_kernel_builds_host_tools` (real
  index, both NuttX boards, host lacking both injected) and
  `a_source_asks_for_the_system_keys_its_build_needs` — both fail with the plan
  arm disabled; the former also fails with the index row emptied.
- `nros setup qemu-armv7a-nuttx --dry-run` (and `rv-virt-nuttx`) on a PATH
  with no `unzip` and a `python3` that cannot import `ensurepip`:

```
  python3-venv   west + kconfiglib tool venvs (issue 0368 F7) [needed by nuttx-kernel]
  unzip          unpacks ... the dtc/libmetal archives the NuttX kernel build downloads ... [needed by nuttx-kernel]
Install with (or re-run with --sudo):
  sudo apt-get install -y python3-venv unzip || { ... }
```
