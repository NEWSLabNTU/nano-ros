# shellcheck shell=bash
# PROBE-OWNED verification for the `upgrade-<lang>` tracks (issue 2912).
# Appended after the book-extracted steps by run-bootstrap-probe.sh; runs in
# the same shell, in the project upgrade-before.sh made and the release page's
# Upgrade block moved.
#
# The page's last step (`nros build`) already had to print its documented
# output. What is left is the state the upgrade must leave: the pin names
# the candidate, the contract carries no spelling the candidate refuses, and,
# for a C or C++ image, the build lives in the image's own directory and the
# previous release's directory no longer holds a configure.

echo '=== probe verify: the release page upgraded a project made by the previous release ==='
: "${PROBE_UPGRADE_FROM:?upgrade-before.sh sets it; the install line of the page did not call it}"
: "${PROBE_UPGRADE_LANG:?upgrade-before.sh sets it}"
: "${PROBE_UPGRADE_DIR:?upgrade-before.sh sets it}"
: "${PROBE_EXPECT_PIN:?run-bootstrap-probe.sh sets it (the candidate the page pins)}"
cd "$PROBE_UPGRADE_DIR" || exit 1

rc=0
nros_grep_q "^version = \"${PROBE_EXPECT_PIN}\"\$" nros-toolchain.toml || rc=$?
case "$rc" in
    0) ;;
    1)
        echo "PROBE FAIL: nros-toolchain.toml does not pin ${PROBE_EXPECT_PIN} after the page's \`nros pin\`:" >&2
        cat nros-toolchain.toml >&2
        exit 1
        ;;
    *)
        echo "PROBE FAIL: could not read nros-toolchain.toml (grep exited $rc)" >&2
        exit 1
        ;;
esac

contract=""
for c in src/*_bringup/system.contract.yaml; do [ -f "$c" ] && { contract="$c"; break; }; done
if [ -z "$contract" ]; then
    echo "PROBE FAIL: no src/<bringup>/system.contract.yaml in the upgraded project" >&2
    exit 1
fi
rc=0
nros_grep_q 'rate_hz' "$contract" || rc=$?
case "$rc" in
    1) ;;
    0)
        echo "PROBE FAIL: $contract still writes rate_hz after \`nros configure --migrate\`:" >&2
        cat "$contract" >&2
        exit 1
        ;;
    *)
        echo "PROBE FAIL: could not read $contract (grep exited $rc)" >&2
        exit 1
        ;;
esac

# The page's steps are plain steps: each had to exit 0, so `nros build` ran to
# its end. Its image must be there, and newer than the pin the page moved.
# Globs, not `ls | head`: under pipefail a closed pipe is a silent exit.
artifact=""
if [ "$PROBE_UPGRADE_LANG" = rust ]; then
    set -- build/*/linux_entry/target/*/linux_entry
else
    set -- build/*-linux/cmake/linux_entry
fi
for a in "$@"; do
    [ -f "$a" ] || continue
    if [ -z "$artifact" ] || [ "$a" -nt "$artifact" ]; then artifact="$a"; fi
done
if [ -z "$artifact" ] || [ ! nros-toolchain.toml -ot "$artifact" ]; then
    echo "PROBE FAIL: no linux_entry built after the page moved the pin (found: '${artifact:-none}')" >&2
    exit 1
fi

if [ "$PROBE_UPGRADE_LANG" != rust ]; then
    bringup="$(basename "$(dirname "$contract")")"
    new_root=""
    for r in build/*-"$bringup"-linux; do [ -d "$r" ] && { new_root="$r"; break; }; done
    if [ -z "$new_root" ] || [ ! -f "$new_root/CMakeLists.txt" ]; then
        echo "PROBE FAIL: no per-image build root build/<coord>-$bringup-linux/ after the upgrade:" >&2
        ls build >&2
        exit 1
    fi
    for old in build/*-"$bringup"; do
        [ -d "$old" ] || continue
        if [ -e "$old/CMakeLists.txt" ] || [ -e "$old/cmake" ]; then
            echo "PROBE FAIL: $PROBE_UPGRADE_FROM's build root $old still holds a configure after the upgrade" >&2
            ls -la "$old" >&2
            exit 1
        fi
    done
fi
echo "=== probe verify: PASS ($PROBE_UPGRADE_LANG project from $PROBE_UPGRADE_FROM, now pinned to $PROBE_EXPECT_PIN) ==="
