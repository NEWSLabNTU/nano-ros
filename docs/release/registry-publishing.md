# Registry Publishing — Per-Ecosystem Cheatsheet (Phase 139.8)

This doc enumerates how the Phase 139 integration shells reach
end users via each RTOS ecosystem's package registry. **It is
docs-only.** Actual `upload` / `publish` invocations are not
automated in CI; credentials live with designated maintainers, not
in the repo.

## Relation to local-development tiers

Downstream consumers of a packaged nano-ros release pull a pinned
version through their RTOS's package manager (the rest of this
doc). They never invoke `just esp_idf setup` / `just px4 setup`
themselves — the RTOS toolchain handles the SDK install. The
[`just setup` tier system](../development/sdk-tiers.md) (Phase 142)
applies to **local development** of nano-ros itself, not to
downstream consumers.

Each section below covers:

1. Registry (what + URL)
2. Files in `integrations/<rtos>/` that drive the upload
3. Auth model (who owns the credential)
4. Reference command (do NOT run from a shared shell)
5. Verification (what to check after a release)

## Zephyr — west manifest

**Registry.** There is no central Zephyr module registry. Modules
are discovered via downstream workspaces that `import:` a
`west.yml` fragment.

**Files.** `zephyr/west.yml` (manifest fragment),
`zephyr/module.yml` (discovery marker).

**Auth model.** Downstream workspaces clone nano-ros directly from
GitHub; auth is whatever the downstream's git transport uses (https
public read, or a deploy key for private mirrors).

**Reference command.** No publishing command. Document the import
snippet in the project README for downstream consumers.

**Verification.** A test workspace with `west update` against the
published `west.yml` should produce a tree containing
`modules/nano-ros/zephyr/module.yml`.

## ESP-IDF — Espressif Component Registry

**Registry.** [components.espressif.com](https://components.espressif.com).
Namespace `<owner>/<name>`. Suggested: publish as
`nano-ros/nano-ros` (request the namespace from Espressif first
release).

**Files.** `integrations/nano-ros/idf_component.yml` (manifest;
includes `version`, `description`, `url`, `license`,
`dependencies`).

**Auth model.** ESP Component Registry uses an API token
(`IDF_COMPONENT_API_TOKEN` env var). Token owned by a designated
nano-ros maintainer with publish rights to the namespace. Rotation:
per IDF Component Manager docs.

> **Decision (2026-07-15, issue #198 — wontfix): the registry publish is NOT
> performed.** Documented source consumption is the ESP-IDF contract: clone
> nano-ros at a pinned tag, run `scripts/bootstrap.sh`, and point the consumer
> `main/idf_component.yml` at
> `path: "../components/nano-ros/integrations/nano-ros"` (the snippet in that
> manifest's header; e2e-tested by `cli_bringup_esp_idf`).
>
> Why: (1) a pack of `integrations/nano-ros/` contains ONLY the three shell
> files — its `_nros_root = ${CMAKE_CURRENT_LIST_DIR}/../..` escapes the
> archive, so a registry-installed copy breaks unconditionally (verified with
> `compote component pack`); (2) even a whole-tree pack cannot be turnkey —
> the build needs a host Rust toolchain + the bootstrap-built `nros` CLI, and
> message bindings are generated per-consumer; (3) a manifest `git:`
> dependency would recurse all 23 submodules (the component manager's fetcher
> hardcodes `with_submodules=True`), a multi-GB first fetch. Precedent:
> micro-ROS's ESP-IDF component is git-consumed, not registry-published.
>
> Revisit if Espressif adds submodule filtering to git sources, or if
> registry discoverability becomes a goal (then: a thin self-fetching shell —
> see archived issue 0198). If a publish is ever attempted, note that
> `idf.py upload-component` is DEPRECATED (IDF 5.3) — the canonical flow is
> `compote component pack` (dry-run) / `compote component upload`.

## PlatformIO — PlatformIO Registry

**Registry.** [registry.platformio.org](https://registry.platformio.org).
Publishing is via `pio package publish`.

**Files.** `library.json` at the **repo root** — that is the PIO manifest
(its `build.extraScript` points at `integrations/platformio/nros_codegen.py`,
which is the only file under `integrations/platformio/`). There is no
`integrations/platformio/library.json` and no `library.properties`: the
Arduino-style sibling was never written, and `arduino` was removed from the
manifest's `frameworks` (#171).

**Auth model.** Requires `pio account login` on the maintainer's
machine, using PlatformIO account credentials. Tokens persist in
`~/.platformio/`. Owned by a designated nano-ros maintainer.

**Reference command (do NOT run from shared shell):**

```bash
# Maintainer machine, after `git tag v0.X.Y`. Run from the repo root — that is
# where `library.json` lives.
pio package publish --type library
```

> **Never executed.** No release has ever run this; there is no CI for it. Treat
> the command as untested until a maintainer performs the first publish (#171).

**Verification.** A test PIO project with `lib_deps = nano-ros@^0.X.Y`
in `platformio.ini` and `pio run` should pull the library and
succeed against at least the `native` platform.

## NuttX — git submodule / `apps/external/`

**Registry.** No central registry. NuttX apps under
`apps/external/<name>/` are discovered when the directory exists
and `Make.defs` declares the app.

**Files.** `integrations/nuttx/{Make.defs,Makefile,Kconfig,CMakeLists.txt}`.

**Auth model.** Same as Zephyr: downstream workspaces clone
nano-ros via their git transport.

**Reference command.** No publishing command. README documents the
symlink / submodule pattern:

```bash
ln -s /path/to/nano-ros/integrations/nuttx \
      $NUTTX_DIR/../apps/external/nano-ros
```

**Verification.** `make menuconfig` in a NuttX checkout with the
symlink in place must surface `nano-ros ROS 2 client` under
`Application Configuration → External Modules`.

## PX4 — `EXTERNAL_MODULES_LOCATION`

**Registry.** No central registry. PX4 external modules are
discovered via the `EXTERNAL_MODULES_LOCATION` env var.

**Files.** `integrations/px4/module-template/` (copy-out template;
not vendored automatically — downstreams `cp -r` or vendor as a
git submodule).

**Auth model.** Same as Zephyr / NuttX: downstream git transport.

**Reference command.** No publishing command. README documents:

```bash
cp -r /path/to/nano-ros/integrations/px4/module-template ./my-px4-modules
export EXTERNAL_MODULES_LOCATION=$PWD/my-px4-modules
```

**Verification.** `make -C $PX4_AUTOPILOT_DIR px4_sitl_default`
with `EXTERNAL_MODULES_LOCATION=…` must produce a binary listing
the user-renamed module under `pxh> help`.

---

## Cross-ecosystem release checklist

Per release cycle, the maintainer doing the cut should:

1. Bump `version` in:
   - `integrations/nano-ros/idf_component.yml`
   - `library.json` (repo root — the PlatformIO manifest)
2. Tag the repo: `git tag v0.X.Y && git push --tags`.
3. Publish to the centralised registries only:
   - ESP Component Registry (`idf.py upload-component`)
   - PlatformIO Library Registry (`pio package publish`)
4. Verify via the per-ecosystem "Verification" steps above.
5. Zephyr / NuttX / PX4 require no publish action — the git tag is
   the release surface; downstreams update their pin.

Credential ownership and per-release CI integration are open
questions for Phase 140 / post-1.0; until then this checklist is
the runbook.
