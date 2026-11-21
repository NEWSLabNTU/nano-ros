#!/usr/bin/env bash
# Fetch ONE Zephyr module that nano-ros's west allowlist deliberately leaves
# out, into a directory of the caller's choosing, for one build (issue 2811).
#
#   scripts/zephyr/fetch-job-module.sh <module> <dest> [--zephyr-base DIR]
#
# WHY. `nros-sdk-index.toml` declares some modules with `lines = []`: known,
# not fetched, so no fresh workspace pays for them (`hal_nxp` is 1.3 GB). A
# board whose SoC needs one (the S32Z270 RTU0 needs `hal_nxp`) can still be
# BUILT by a job that fetches the module for itself, here, and names it to the
# configure (`EXTRA_ZEPHYR_MODULES`). The shared workspace is never touched:
# the module lands in `<dest>`, which the caller keeps job-local (CI) or
# checkout-local (a developer), never under the store workspace.
#
# WHICH REVISION. The one the workspace's own Zephyr pins (`zephyr/west.yml`),
# so the module matches the tree it builds against, exactly as `west update`
# would have fetched it. Fetched shallow, blobless and sparse: only the paths
# the module's build reads for the boards that need it (`SPARSE_EXCLUDES`
# below), so `hal_nxp` costs tens of MB, not 1.3 GB.
#
# Idempotent: a `<dest>` already at the pinned revision is reused. Prints the
# module directory on stdout; everything else goes to stderr.
set -euo pipefail

usage() {
    sed -n '2,22p' "${BASH_SOURCE[0]}" >&2
    exit 2
}

module="${1:-}"
dest="${2:-}"
[ -n "$module" ] && [ -n "$dest" ] || usage
shift 2
zephyr_base="${ZEPHYR_BASE:-}"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --zephyr-base) zephyr_base="${2:-}"; shift 2 ;;
        -h | --help) usage ;;
        *) echo "fetch-job-module: unknown argument $1" >&2; exit 2 ;;
    esac
done

# The paths a sparse checkout drops, per module. Only what the boards that
# need the module never read: hal_nxp's top-level CMakeLists.txt takes `s32/`
# when CONFIG_HAS_NXP_S32_HAL is set and `imx/` only otherwise, and `s32/`
# carries its own `mcux/devices/` (it reads `mcux/hal_nxp.cmake` and the
# common `mcux/mcux-sdk/drivers/`, never `mcux/mcux-sdk/devices/`, which is
# 749 MB of the 880 MB tree). A module not listed is fetched whole.
declare -A SPARSE_EXCLUDES=(
    [hal_nxp]="mcux/mcux-sdk/devices imx"
)

if [ -z "$zephyr_base" ] || [ ! -f "$zephyr_base/west.yml" ]; then
    echo "fetch-job-module: no Zephyr tree (ZEPHYR_BASE or --zephyr-base): '$zephyr_base'" >&2
    echo "  remedy: just zephyr setup" >&2
    exit 1
fi

# `<name> <url> <revision>` from the Zephyr tree's own manifest: the
# project's remote url-base (or the manifest default) + `repo-path` or name.
# A command substitution, so the reader's exit status is the assignment's.
project="$(python3 - "$zephyr_base/west.yml" "$module" <<'PY'
import sys
try:
    import yaml
except ImportError:
    sys.exit("fetch-job-module: PyYAML missing (the Zephyr venv carries it: "
             "source scripts/build/zephyr-python.sh; nros_zephyr_activate)")
doc = yaml.safe_load(open(sys.argv[1]))["manifest"]
want = sys.argv[2]
remotes = {r["name"]: r["url-base"] for r in doc.get("remotes", [])}
default = (doc.get("defaults") or {}).get("remote")
for p in doc.get("projects", []):
    if p.get("name") != want:
        continue
    base = remotes.get(p.get("remote", default))
    if not base or "revision" not in p:
        sys.exit(f"fetch-job-module: {want}: no remote or revision in the manifest")
    print(want, f"{base}/{p.get('repo-path', want)}", p["revision"])
    break
else:
    sys.exit(f"fetch-job-module: {want} is not a project of {sys.argv[1]}")
PY
)" || exit 1
read -r name url rev <<< "$project"
[ -n "${rev:-}" ] || exit 1

if [ -e "$dest/.git" ] && [ "$(git -C "$dest" rev-parse HEAD 2>/dev/null)" = "$rev" ]; then
    echo "fetch-job-module: $name already at $rev in $dest" >&2
    printf '%s\n' "$dest"
    exit 0
fi

echo "fetch-job-module: $name $rev from $url -> $dest" >&2
rm -rf "$dest"
mkdir -p "$dest"
git -C "$dest" init -q
git -C "$dest" remote add origin "$url"
excludes="${SPARSE_EXCLUDES[$name]:-}"
if [ -n "$excludes" ]; then
    patterns=('/*')
    for d in $excludes; do patterns+=("!/$d/"); done
    # `init --no-cone` first: git before 2.35 reads a `set --no-cone` flag
    # as one more pattern.
    git -C "$dest" sparse-checkout init --no-cone
    git -C "$dest" sparse-checkout set "${patterns[@]}"
fi
git -C "$dest" fetch -q --depth 1 --filter=blob:none origin "$rev"
git -C "$dest" -c advice.detachedHead=false checkout -q FETCH_HEAD
[ -f "$dest/zephyr/module.yml" ] || {
    echo "fetch-job-module: $dest has no zephyr/module.yml after checkout" >&2
    exit 1
}
echo "fetch-job-module: $name ready ($(du -sh "$dest" | cut -f1))" >&2
printf '%s\n' "$dest"
