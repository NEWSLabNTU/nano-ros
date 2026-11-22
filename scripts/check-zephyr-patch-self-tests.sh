#!/usr/bin/env bash
# The Zephyr patch scripts that carry a `--self-test` pass it (issue 2867).
#
# `cargo-features-patch.sh`'s self-test failed at main for an unknown time,
# because no lane ran it: a negative control nobody runs decays into a
# comment. This gate runs each script's self-test, and first proves each one
# can still fail: a copy of the script with the defect its test exists for
# put back must FAIL its own self-test. Buildless: stub files in a tmpdir.
#
#   bash scripts/check-zephyr-patch-self-tests.sh
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# script -> the defect its self-test must catch, as `<old>` => `<new>` applied
# to a copy. Python does the replacement so neither side needs escaping.
declare -A MUTATION=(
    # The staticlib hunk stops making the build command staticlib-only.
    [scripts/zephyr/cargo-features-patch.sh]='"CARGO_ARGS rustc --crate-type staticlib")'$'\t''"CARGO_ARGS rustc")'
    # The applied-file guard goes, so an applied file is written back.
    [scripts/zephyr/tcm-section-align-patch-4.4.sh]='    if changed == before:'$'\n''        continue'$'\t''    if changed == before and os.environ.get("NROS_PATCH_CHECK") == "1":'$'\n''        continue'
)

# A copy of the checkout's scripts/ with <script> mutated, at <dir>.
mutated_copy() {
    local script="$1" dir="$2" spec="${MUTATION[$1]}"
    mkdir -p "$dir/scripts/zephyr" "$dir/scripts/lib"
    cp "$repo/scripts/lib/zephyr-patchset.sh" "$repo/scripts/lib/grep-q.sh" "$dir/scripts/lib/"
    python3 - "$repo/$script" "$dir/$script" "${spec%%$'\t'*}" "${spec#*$'\t'}" <<'PYEOF'
import sys
src, dst, old, new = sys.argv[1:]
text = open(src).read()
if text.count(old) != 1:
    sys.exit(f"the mutation's anchor is not in {src} exactly once: {old!r}")
open(dst, "w").write(text.replace(old, new, 1))
PYEOF
}

# The negative controls, on every run: each mutated copy fails its self-test.
selftest() {
    local script rc
    for script in "${!MUTATION[@]}"; do
        mutated_copy "$script" "$tmp/neg-$(basename "$script")"
        rc=0
        bash "$tmp/neg-$(basename "$script")/$script" --self-test >/dev/null 2>&1 || rc=$?
        if [ "$rc" -eq 0 ]; then
            echo "check-zephyr-patch-self-tests: FAIL — $script's self-test passes with" >&2
            echo "  the defect it exists to catch put back, so it can no longer fail." >&2
            return 1
        fi
    done
}

selftest
for script in "${!MUTATION[@]}"; do
    if ! bash "$repo/$script" --self-test; then
        echo "check-zephyr-patch-self-tests: FAIL — $script --self-test" >&2
        exit 1
    fi
done
echo "check-zephyr-patch-self-tests: OK — ${#MUTATION[@]} patch script self-test(s) pass," \
    "and each fails with its defect put back"
