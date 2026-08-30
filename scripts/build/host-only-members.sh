#!/usr/bin/env bash
# Emit `--exclude <crate>` for every workspace member declared HOST-ONLY.
#
# issue 0287. `just check workspace-embedded` builds the whole workspace for a
# thumb target, and cargo unifies features across ALL members regardless of what
# firmware can actually reach. So one host-only member — even an orphan nothing
# deps — turns `std` on for every crate and the lane dies somewhere unrelated:
#
#     error[E0463]: can't find crate for `std`
#       --> packages/core/nros-serdes/src/lib.rs:31:1
#
# The error names a crate that is fine, which is what makes this expensive to
# diagnose. `nros-serdes` is simply the first no_std crate cargo got to.
#
# That was handled by a hand-written `--exclude` list in the justfile: 20 lines,
# no reasons, and nothing tying an entry to the crate it excludes. A list like
# that only stays correct while someone remembers it exists. Now each crate
# declares itself:
#
#     [package.metadata.nros]
#     host-only = true
#     host-only-reason = "bindgen + C build of zenoh-pico; needs a host toolchain"
#
# and this script derives the flags. The reason lives next to the crate, and
# adding a host-only crate is one edit in the place you are already editing.
#
# Reads the git index rather than walking: `Cargo.toml` is tracked, so this is
# an index lookup (see scripts/check-no-tracked-file-find.sh for the measurement
# that made that the rule here).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

names=()
while IFS= read -r manifest; do
    # `host-only = true` as a KEY, not the string anywhere in the file: several
    # of these manifests mention "host-only" in prose, and matching that
    # silently skipped three crates when this was first written.
    grep -qE '^host-only[[:space:]]*=[[:space:]]*true' "$manifest" || continue
    name="$(grep -m1 -E '^name[[:space:]]*=' "$manifest" | sed -E 's/.*"([^"]+)".*/\1/')"
    [ -n "$name" ] || continue
    names+=("$name")
done < <(git ls-files 'packages/**/Cargo.toml')

if [ "${#names[@]}" -eq 0 ]; then
    # Fail loudly. Emitting nothing would run the embedded lane over every
    # host-only crate and produce the confusing E0463 this script exists to
    # prevent — a silent empty result is the worst outcome available.
    echo "host-only-members: found NO crates declaring [package.metadata.nros] host-only = true." >&2
    echo "  The embedded lane needs these excluded; refusing to emit an empty list." >&2
    exit 1
fi

printf -- '--exclude %s\n' "${names[@]}" | sort | tr '\n' ' '
