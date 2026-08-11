#!/usr/bin/env bash
# phase-340 W2.d — `--core-only` selects by the DERIVED variant predicate, and
# that must stay equivalent to the authored-`target_dir` spelling it replaced
# WHERE IT IS CONSUMED.
#
# The equivalence is a fact about today's manifest, not a law: the two spellings
# diverge on `qemu-arm-nuttx` rust rows, which carry configuration and no
# authored dir. `--core-only` has exactly one caller
# (`just/native.just` -> `fixtures-build.sh linux rust --core-only`), and that
# caller never selects nuttx, so the divergence is invisible — TODAY.
#
# This gate makes it visible tomorrow. It fails when either
#   * a caller starts using `--core-only` on a platform where the spellings
#     differ, or
#   * a row is added that the two spellings classify differently on a platform
#     that IS consumed.
# Without it, deleting the `target_dir` column silently changes host-integration
# lane membership at some later date, which is the risk that kept W2.d open.
set -uo pipefail
cd "$(dirname "$0")/../../../.."

M=scripts/build/fixtures-manifest.py
fail=0

# Every platform any caller passes with --core-only. Derived from the tree, not
# a literal, so a new caller joins the check automatically.
consumed="$(grep -rhoE '[a-z0-9-]+ +[a-z]+ +--core-only' just/ scripts/ 2>/dev/null \
    | awk '{print $1}' | sort -u)"
[ -n "$consumed" ] || { echo "FAIL: no --core-only caller found — has the flag been removed?" >&2; exit 1; }

for plat in $consumed; do
    derived="$(python3 "$M" list --platform "$plat" --lang rust --core-only 2>/dev/null | wc -l)"
    authored="$(python3 - "$plat" <<'PY'
import sys
try: import tomllib
except ModuleNotFoundError: import tomli as tomllib
plat = sys.argv[1]
d = tomllib.load(open("examples/fixtures.toml", "rb"))
rows = [e for e in d.get("fixture", [])
        if e.get("lang") == "rust" and e.get("platform") == plat]
print(sum(1 for e in rows if not e.get("target_dir")))
PY
)"
    # A platform with no rows makes both counts 0, which "agrees" and proves
    # nothing — found by a tripwire that used `qemu-arm-nuttx` where the
    # manifest's token is `nuttx`, and passed. An empty selection is a bug in
    # the caller or a stale token, either way not a pass.
    if [ "$derived" -eq 0 ] && [ "$authored" -eq 0 ]; then
        echo "FAIL: --core-only names platform '$plat', which has no rust rows." >&2
        echo "      Both spellings select 0, so this check would prove nothing." >&2
        echo "      Fix the caller's platform token, or drop the caller." >&2
        fail=1
        continue
    fi
    if [ "$derived" != "$authored" ]; then
        echo "FAIL: --core-only on '$plat' selects $derived rows by the derived predicate" >&2
        echo "      but $authored by the authored-target_dir spelling it replaced." >&2
        echo "      Deleting the column would change this lane's membership." >&2
        echo "      Decide which is correct — do not silently take the new number." >&2
        fail=1
    else
        echo "  ok   $plat: $derived row(s), both spellings agree"
    fi
done

[ "$fail" -eq 0 ] || exit 1
echo "core-only predicate: derived and authored spellings agree on every consumed platform"
