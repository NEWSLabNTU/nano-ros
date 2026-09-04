set working-directory := '..'

# Verification module — install and run formal verification tools (Kani + Verus).

# The Kani version this repo's proofs are verified with. PINNED, and low on
# purpose: 0.62.0's `kani-driver` needs only GLIBC_2.34, so it runs on Ubuntu
# 22.04 (glibc 2.35) and 24.04 alike. 0.67.0's needs GLIBC_2.39 — 24.04 only —
# and it is the ONLY binary in that bundle over the line, so the failure looks
# like a broken toolchain rather than one over-linked entry point (issue 0802).
#
# `cargo install --locked` does NOT pin this: `--locked` pins the crate's own
# dependency graph, not which version you get. Without the `--version` below you
# get whatever crates.io published today, which makes "the harnesses verify" a
# claim about an unrecorded prover.
#
# Raising it is a decision (drops 22.04), not a default. Re-check with:
#   objdump -T ~/.kani/kani-<ver>/bin/kani-driver | grep -o 'GLIBC_2\.[0-9]*' | sort -V | tail -1
KANI_VERSION := "0.62.0"

default:
    @just --list --justfile {{source_file()}}

# Install Kani + Verus.
[group("setup")]
setup:
    @just verification kani
    @just verification verus

# Diagnose verification tool install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    # Ask the driver, not the filesystem — a ~/.kani directory can be stale or
    # unrunnable and still look installed (issue 0802).
    kani_ver=$(cargo kani --version 2>/dev/null | awk '{print $NF}' || true)
    if [ -z "$kani_ver" ]; then
        if [ -d "$HOME/.kani" ]; then
            echo "  [BROKEN] kani-verifier is installed but will not run (glibc? see issue 0802)"
        else
            echo "  [MISSING] kani-verifier (run: just verification kani)"
        fi
        fail=1
    elif [ "$kani_ver" != "{{KANI_VERSION}}" ]; then
        echo "  [WRONG] kani-verifier $kani_ver, this repo pins {{KANI_VERSION}} (run: just verification kani)"
        fail=1
    else
        echo "  [OK] kani-verifier: $kani_ver (pinned)"
    fi
    # phase-422 W2 — Verus is an SDK-store tool (`[tool.verus]`), so its path is
    # CONSTRUCTED from the index pin and never searched for (issue 0625). No
    # `--require`: a doctor reports, it does not abort.
    verus_bin="$(nros sdk-path verus 2>/dev/null)/bin/verus"
    if [ -x "$verus_bin" ] || command -v verus &>/dev/null; then
        ver=$("$verus_bin" --version 2>/dev/null | grep -i Version | head -1 | tr -d ' ' || echo "")
        echo "  [OK] verus${ver:+ ($ver)}"
    else
        echo "  [MISSING] verus (run: just verification verus)"
        fail=1
    fi
    exit $fail

# Install Kani bounded model checker (skipped if already installed).
[group("verification")]
kani:
    #!/usr/bin/env bash
    set -e
    want="{{KANI_VERSION}}"
    # A directory under ~/.kani is NOT evidence the right prover is installed:
    # the store ACCUMULATES, so a stale bundle shadows the pin exactly as issue
    # 0500's Corrosion prefixes do. Ask the driver what it is, and make it prove
    # it can run at all — 0802 was a bundle that installed fine and could not
    # execute.
    have=$(cargo kani --version 2>/dev/null | awk '{print $NF}' || true)
    if [ "$have" = "$want" ]; then
        echo "kani-verifier $have already installed (pinned)"
    else
        if [ -n "$have" ]; then
            echo "kani-verifier $have installed, but this repo pins $want — replacing."
        fi
        cargo install --locked --version "$want" kani-verifier
        cargo kani setup
        got=$(cargo kani --version 2>/dev/null | awk '{print $NF}' || true)
        if [ "$got" != "$want" ]; then
            echo "ERROR: installed kani-verifier reports '${got:-nothing}', wanted $want." >&2
            echo "       If it reports nothing, it installed but cannot RUN — check glibc:" >&2
            echo "       objdump -T ~/.kani/kani-$want/bin/kani-driver | grep -o 'GLIBC_2\.[0-9]*' | sort -V | tail -1" >&2
            exit 1
        fi
    fi

# Provision the pinned Verus release into the SDK store, plus its rust toolchain.
#
# phase-422 W2 — the DOWNLOAD is the index's job now: `[tool.verus]` pins the
# release, names the per-host asset and carries its sha256. The retired
# `scripts/setup-verus.sh` resolved its asset through the GitHub releases API
# (`latest` on request, a tag by default) and verified NO checksum, so what it
# fetched was a query answered at run time rather than a fact recorded in the
# tree — which is precisely what a pin is for, and the reason that script had
# already had to hardcode a release once (issue 0368 F6).
#
# What stays here is the half the index cannot express: the rustup toolchain the
# release pins, and the informative degrade for a host that cannot run the
# binary at all.
[group("verification")]
verus:
    #!/usr/bin/env bash
    set -euo pipefail
    nros setup --tool verus
    verus_bin="$(nros sdk-path verus --require)/bin/verus"

    # `verus --version` fails for TWO distinct reasons and they need OPPOSITE
    # handling: a missing pinned rust toolchain (install it — the normal
    # fresh-install path) vs a loader/glibc error (degrade informatively —
    # installing anything won't help). Disambiguate on the error text.
    #
    # Matched with bash's own `case`, not `grep -q`: a `grep` that fails to
    # START exits >=2, which reads here exactly like "no match" and would send a
    # glibc failure down the toolchain-install path (issue 0726's class).
    if out="$("$verus_bin" --version 2>&1)"; then
        echo "$out"
        exit 0
    fi
    case "$out" in
        *"required rust toolchain"*)
            # Two formats: an INSTALLED-toolchain verus prints "Toolchain: <tc>";
            # a FRESH one cannot print --version at all and errors "required rust
            # toolchain <tc> not found". Parse both — phase-327 W6: the
            # fresh-install path always hit the second and installed nothing.
            tc=$(printf '%s\n' "$out" \
                | sed -n -e 's/.*Toolchain: //p' -e 's/.*required rust toolchain \([^ ]*\) not found.*/\1/p' \
                | head -1)
            if [ -z "$tc" ]; then
                echo "[verus] could not parse the required rust toolchain from:" >&2
                printf '%s\n' "$out" >&2
                exit 1
            fi
            if rustup run "$tc" rustc --version >/dev/null 2>&1; then
                echo "Required toolchain already installed: $tc"
            else
                echo "Installing required toolchain: $tc"
                rustup toolchain install "$tc"
            fi
            "$verus_bin" --version
            ;;
        *)
            # Issue 0368 F6. Verification is gated in no CI tier, so a host that
            # cannot run this tool must NOT have its setup fail — exit 0 with an
            # explanation, not a raw loader error.
            printf '%s\n' \
                "[verus] The Verus binary could not run on this host — almost always a" \
                "        glibc mismatch: releases newer than the pin are built against a" \
                "        newer glibc than the oldest supported LTS provides (issue 0368 F6)." \
                "        Verification is not gated in any CI tier, so this is informative," \
                "        not fatal." \
                "        Options: run on a newer host, build Verus from source, or move the" \
                "        pin in [tool.verus] (nros-sdk-index.toml) — it records which release" \
                "        was measured to run, and why." \
                "        It printed:"
            printf '        %s\n' "$out"
            exit 0
            ;;
    esac
