#!/usr/bin/env bash
# Emit `--exclude <crate>` for every workspace member declared EMBEDDED-ONLY.
#
# issue 1315 / phase-451 W4. The MIRROR of `host-only-members.sh`, and it did not
# exist — which is why four real crates were not workspace members at all.
#
# `check::test-targets` runs the workspace clippy for the HOST, and some crates
# cannot build there: a `staticlib`/`cdylib` with no host panic runtime, or a
# crate whose dependencies are `cortex-m` / `esp-hal`. That set was a
# hand-written string, `HOST_UNCHECKABLE` in `just/check.just`:
#
#   HOST_UNCHECKABLE := "--exclude nros-c --exclude nros-cpp --exclude ..."
#
# Eight crates, no reasons, nothing tying an entry to the crate it excluded —
# the exact list issue 0287 retired on the EMBEDDED side, with the argument
# already written in this script's sibling: *"A list like that only stays correct
# while someone remembers it exists."* Nobody remembered. Measured 2026-09-11
# with the per-crate command the lane itself runs, 5 of the 8 were STALE — clean
# on the host and excluded anyway.
#
# The cost was not the five. It was that `host-only` had no mirror, so a crate
# that cannot build for the host had no way to say so and stay a member: the
# only way to keep it out of the host lane was to keep it out of the WORKSPACE,
# where no lane compiles, lints, tests or formats it (issue 1309).
#
#     [package.metadata.nros]
#     embedded-only = true
#     embedded-only-reason = "cortex-m HAL; no host build"
#
# Reads the git index, not a walk (`check-no-tracked-file-find`).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

names=()
while IFS= read -r manifest; do
    # The KEY, not the string anywhere in the file — these manifests discuss
    # "embedded-only" in prose, and matching that is how the sibling script
    # silently skipped three crates on its first version.
    grep -qE '^embedded-only[[:space:]]*=[[:space:]]*true' "$manifest" || continue
    # `|| true` — a manifest with no `name =` is skipped by the `[ -n ]` below;
    # grep's exit 1 under pipefail would end the whole sweep instead (issue 1249).
    name="$(grep -m1 -E '^name[[:space:]]*=' "$manifest" | sed -E 's/.*"([^"]+)".*/\1/' || true)"
    [ -n "$name" ] || continue
    names+=("$name")
done < <(git ls-files 'packages/**/Cargo.toml')

if [ "${#names[@]}" -eq 0 ]; then
    # Fail loudly, for the sibling's reason inverted: emitting nothing would run
    # the HOST lane over crates that cannot build there, and the failure would
    # name a crate nobody touched.
    echo "embedded-only-members: found NO crates declaring [package.metadata.nros] embedded-only = true." >&2
    echo "  The host lane needs these excluded; refusing to emit an empty list." >&2
    exit 1
fi

printf -- '--exclude %s\n' "${names[@]}" | sort | tr '\n' ' '
