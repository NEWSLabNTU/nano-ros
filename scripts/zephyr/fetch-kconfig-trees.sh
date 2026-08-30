#!/usr/bin/env bash
# Fetch the Zephyr 4.4 SOURCE the Kconfig symbol gate needs — issue 0651.
#
# `check-zephyr-kconfig-symbols.py` answers "does every symbol `zephyr/Kconfig`
# names exist on every supported line" from SOURCE, no build. 3.7 comes from the
# workspace `just zephyr setup` already makes. 4.4 came from nothing: it lives in
# a sibling west workspace nobody has, so on a normal dev host the gate checked
# 3.7, printed OK, and noted that it had not looked at the line it exists for.
#
# That is the failure mode issue 0651 names as the one to design against — "a
# lane that skips when unprovisioned and reports the same colour as a lane that
# passed" — so the gate is now STRICT, and strictness needs the remedy to be one
# command. This is that command.
#
# Two bare clones, not a west workspace. Kconfig lives in the `zephyr` repo plus
# the modules west pins, and of those only `zephyr-lang-rust` defines symbols
# this repo references (`RUST`). A full 4.4 west workspace is ~20 modules and
# filled a CI disk once already (issue 0078).
#
# The revisions are READ FROM `west-4.4.yml`, never restated here: a pin copied
# into a second file is a pin that can disagree with the manifest, and this repo
# has paid for that spelling more than once.
#
# Idempotent. Re-running with the trees present and at the pinned revisions does
# nothing; `--force` re-fetches.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MANIFEST="$ROOT/west-4.4.yml"
DEST="$ROOT/build/zephyr-kconfig"
FORCE=""
[ "${1:-}" = "--force" ] && FORCE=1

[ -f "$MANIFEST" ] || { echo "error: no $MANIFEST" >&2; exit 1; }

# `<project>@<revision>` for the two projects, straight out of the manifest.
pins() {
    python3 - "$MANIFEST" <<'PY'
import sys, yaml
m = yaml.safe_load(open(sys.argv[1]))["manifest"]
want = {"zephyr": "zephyr-4.4", "zephyr-lang-rust": "zephyr-lang-rust-4.4"}
by = {p["name"]: p for p in m["projects"]}
for name, dirname in want.items():
    if name not in by:
        sys.stderr.write(f"error: {name} is not a project in west-4.4.yml\n")
        sys.exit(1)
    rev = by[name].get("revision")
    if not rev:
        sys.stderr.write(f"error: {name} has no revision in west-4.4.yml\n")
        sys.exit(1)
    print(f"{dirname}\t{name}\t{rev}")
PY
}

fetch_one() { # <dirname> <project> <revision>
    local dir="$DEST/$1" name="$2" rev="$3"
    local url="https://github.com/zephyrproject-rtos/$name"

    if [ -n "$FORCE" ] && [ -d "$dir" ]; then
        echo "[kconfig-trees] --force: removing $dir"
        rm -rf "$dir"
    fi

    if [ -d "$dir/.git" ]; then
        local at
        at="$(git -C "$dir" rev-parse HEAD 2>/dev/null || echo none)"
        # A tag pin resolves to a commit, so compare BOTH spellings.
        if [ "$at" = "$rev" ] || [ "$(git -C "$dir" rev-parse "$rev^{commit}" 2>/dev/null || echo x)" = "$at" ]; then
            echo "[kconfig-trees] $1 already at $rev"
            return 0
        fi
        echo "[kconfig-trees] $1 is at $at, want $rev — refetching"
        rm -rf "$dir"
    fi

    mkdir -p "$DEST"
    echo "[kconfig-trees] cloning $name @ $rev -> ${dir#"$ROOT"/}"
    # A tag can be cloned shallow by name; a bare SHA cannot, so that one needs
    # a fetch of the single object after an empty clone. Both stay shallow.
    if git clone --depth 1 --branch "$rev" --single-branch "$url" "$dir" 2>/dev/null; then
        return 0
    fi
    git clone --filter=blob:none --no-checkout "$url" "$dir"
    git -C "$dir" fetch --depth 1 origin "$rev"
    git -C "$dir" checkout --detach "$rev"
}

while IFS=$'\t' read -r dirname name rev; do
    [ -n "$dirname" ] || continue
    fetch_one "$dirname" "$name" "$rev"
done < <(pins)

echo
echo "[kconfig-trees] ready under ${DEST#"$ROOT"/}:"
for d in "$DEST"/*/; do
    [ -d "$d" ] || continue
    printf '  %-26s %s\n' "$(basename "$d")" "$(git -C "$d" rev-parse --short HEAD 2>/dev/null || echo '?')"
done
echo
echo "Now: just check zephyr-kconfig-symbols"
