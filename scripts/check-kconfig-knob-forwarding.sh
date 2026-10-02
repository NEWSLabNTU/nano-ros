#!/usr/bin/env bash
#
# issue 0460 — every knob Kconfig forwards must be READ by the Rust lane too,
# and it must be read from the RIGHT Kconfig symbol.
#
# # The failure this prevents
#
# `zephyr/cmake/nros_cargo_build.cmake` publishes each tuning knob with
# `set(ENV{<NAME>} ...)`, which only touches the CONFIGURE-time cmake process.
# The C lane survives that because `nros_cargo_build()` re-bakes the variables
# into its build command (`cmake -E env ...`). The RUST lane's command is built
# by zephyr-lang-rust's `rust_cargo_application`, which passes its own fixed
# variable list and inherits nothing — so a Zephyr Rust image compiled its
# crates' DEFAULTS whatever Kconfig said, for EVERY knob at once.
#
# It surfaced as three `workspaces/features` entries dying after "Network
# ready" with a bare `Transport(ServiceServerCreationFailed)`:
# `CONFIG_NROS_MAX_QUERYABLES=16` reached the cmake-compiled shim TU and not
# the cargo-compiled one, which kept the default of 8 while the entries
# registered eleven capability services.
#
# # What this gate asks NOW, and why it is smaller than it was (phase-468 W4)
#
# Until W4 there were EIGHT knob ladders, hand-assembled in eight crates out of
# `knob_usize` / `dotconfig_usize` / a bare `env::var`, and two per-crate
# `KCONFIG_KNOBS` tables. Most of this gate was about holding those eight
# shapes to one another: does this file have a table, does that file call the
# shared helper, is a TABULATING reader's mention of a knob actually a row
# (issue 1490). There is one ladder now — `nros_zephyr_build::knob()` — and one
# pairing table beside it, so those arms are asking about a shape nobody can
# write any more, and they are gone.
#
# What is KEPT is what the reader cannot express, whatever its shape:
#
# 1. **Coverage.** A `knob("X")` call site is evidence about `X` and says
#    nothing about the knob NOBODY named. The 47 forwarded knobs live in a
#    cmake file no Rust code reads, so only a gate can compare the two lists.
#
# 2. **The pairing.** `Knob` derives `CONFIG_<env name>` and consults
#    `KCONFIG_PAIRS` where the two vocabularies differ
#    (`ZPICO_SUBSCRIBER_RING_DEPTH` <-> `CONFIG_NROS_SUBSCRIBER_RING_DEPTH`).
#    Which of those is right for a given knob is the PRODUCER's decision, and
#    the producer writes it down: most `_nros_resolve_knob()` calls pass a
#    literal `"${CONFIG_<SYM>}"`. So the pairing is HARVESTED from cmake and
#    the table is held to it, in both directions — a missing row and a wrong
#    row are equally the 0460 failure, and the reader cannot know either.
#
# 3. **No bare `env::var("<FORWARDED KNOB>")`** (issue 0751). Still writable,
#    still yields the crate default on a Zephyr Rust image, still silent.
#
# Issue 1490 is why arm 2 reads the producer rather than asking each reader for
# a row. Its whole finding was "the name APPEARING is not the name being
# resolved", and the per-crate tables meant one knob could be paired in one
# crate and unpaired in another — `NROS_EXECUTOR_MAX_NODES` was, which is issue
# 1233. A pairing is a property of the KNOB.
#
# # What arm 2 does NOT reach, stated rather than left to be discovered
#
# A knob whose cmake value is a computed variable (`ZPICO_MAX_PUBLISHERS
# "${_nros_zpico_pubs}"`) declares no symbol at the call site, so its row is
# authored and unchecked by arm 2 — 9 of the 19 rows, the four computed
# `_nros_zpico_*` counts, the two tx flags cmake passes as a literal "1"/"0",
# and three the cmake module does not forward at all.
#
# # Arm 1 reads BOTH forwarding helpers (issue 1505, FIXED)
#
# `zephyr/cmake/nros_cargo_build.cmake` has two: `_nros_resolve_knob()` and
# `_nros_resolve_derivable_knob()` ("plus rungs 3 and 4", for a knob whose
# Kconfig option documents `-1` as *derive*). `_nros_resolve_knob(` is **not** a
# substring of `_nros_resolve_derivable_knob(`, so a harvest that reads the
# first spelling misses every knob forwarded by the second, silently — and this
# one did, for 27 of the 74 knobs the module forwards. Its own success line read
# "47 forwarded knob(s)".
#
# That is issue 0196's shape, a REACH narrower than the rule, and it let two
# live splits sit unreported: `ZPICO_MAX_LARGE_SUBSCRIBERS` and
# `ZPICO_SUBSCRIBER_LARGE_SIZE` had a row in neither per-crate table, so a
# Zephyr Rust image read both env-only — and they are two of the three factors
# of the largest pool the tree has.
#
# Widening it was not a one-line change, because 8 of the 27 have no direct
# reader and the reasons are NOT the same. Measured, per knob:
#
#   * The four XRCE ones are read through the `xrce-config.txt` MANIFEST, which
#     arm 1 already models — they need nothing.
#   * The four zenoh `NROS_MAX_*` ones are a RESOLUTION name, not a delivery
#     name: cmake resolves `NROS_RESOLVED_NROS_MAX_<X>` and re-exports it as
#     `ZPICO_MAX_<X>`, and THAT is the name the Rust lane reads. They are in
#     NO_RUST_READER below, each with its re-export line.

set -euo pipefail
cd "$(dirname "$0")/.."

# issue 0726 — the reader-shape check below is the `if ! grep -q` shape, whose
# failure mode is a grep that could not START being reported as "$f never calls
# the resolver": a confident, specific, false claim, and only under load.
# `nros_grep_q` exits 2 on a tool failure instead of returning "no match".
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

CMAKE=zephyr/cmake/nros_cargo_build.cmake
# The crate that owns the ladder AND the pairing table. One file, because there
# is one of each.
RESOLVER=packages/tooling/nros-zephyr-build/src/lib.rs
# Every build script that resolves a forwarded knob. ONE list since phase-468
# W4: the DERIVED/TABULATING split was a property of the two per-crate tables,
# and both are gone.
READERS=(
    packages/api/nros/build.rs
    packages/core/nros-node/build.rs
    packages/core/nros-params/build.rs
    packages/platform/nros-platform/build.rs
    packages/rmw/cffi/build.rs
    packages/rmw/xrce/nros-rmw-xrce-cffi/build.rs
    packages/rmw/zenoh/nros-rmw-zenoh/build.rs
    packages/rmw/zenoh/nros-zpico-build/src/runner.rs
)

# A reader whose env front-end is INJECTED by one of the readers above.
#
# `nros-platform-config` resolves the zenoh tx and wire tenants over the
# RFC-0049 ladder and takes its environment as a parameter
# (`env: &dyn Fn(&str) -> Option<String>`), because it must serve `nros config
# explain` as well as a build. `nros-zpico-build`'s runner hands it
# `Knob::stated_str`, so those seven knobs DO reach `$DOTCONFIG` — but the
# literals live here, one crate away from the call that resolves them.
#
# Each row is `<path>|<token a caller must name>`: the injected reader only
# counts once a listed READER reaches it, exactly as a MANIFEST_READER only
# counts once something parses it. Before phase-468 W4 these seven were covered
# by their rows in the zpico runner's own `KCONFIG_KNOBS` table — which is
# issue 1490's mention test wearing a pairing, and is why the shape is written
# down now rather than re-satisfied by a row.
INJECTED_READERS=(
    "packages/tooling/nros-platform-config/src/platform_config.rs|platform_config::"
)

# phase-420 W9 — a reader may name its knobs in a shared MANIFEST rather than in
# its own source. `packages/rmw/xrce/xrce-config.txt` is the one statement of
# every XRCE build value, read by BOTH the cargo lane (`build.rs`) and the cmake
# lane (`nros-rmw-xrce/CMakeLists.txt`); before it, the CMake lane read none of
# these six options at all and compiled the defaults.
#
# The names appear UNQUOTED there (a whitespace-separated column), so this list
# is matched with a word-boundary grep rather than the `"NAME"` literal the
# source readers use. A manifest only counts once something reads it: the
# consumption check at the bottom proves a READER parses it, and every reader is
# held to the resolver requirement, so the `$DOTCONFIG` rung issue 0751 is about
# is still proved — by the file that resolves the knob, not the file that lists
# it.
MANIFEST_READERS=(
    packages/rmw/xrce/xrce-config.txt
)

# Knobs the cmake side exports that no Rust build script reads. Each needs a
# reason: an unread export is either dead or a C-lane-only knob.
NO_RUST_READER=(
    # The cmake comment says xrce-sys/build.rs reads it unprefixed; that crate
    # was deleted in phase-321 W1.d and the surviving build script uses the
    # `XRCE_TRANSPORT_MTU_DEFAULT` const. C-lane only today.
    XRCE_TRANSPORT_MTU

    # issue 1505 — these four are a RESOLUTION name, never a delivery name, and
    # the distinction is the whole reason they read as unread. cmake resolves
    # each into `NROS_RESOLVED_NROS_MAX_<X>` and then re-exports that value
    # under the `ZPICO_` spelling, which IS what the Rust lane reads and IS
    # checked by this gate under that name. Re-export lines measured
    # 2026-09-28 in `zephyr/cmake/nros_cargo_build.cmake`:
    #
    #   735  ZPICO_MAX_PUBLISHERS      815  ZPICO_MAX_LIVELINESS
    #   739  ZPICO_MAX_QUERYABLES           ZPICO_MAX_SUBSCRIBERS
    #
    # Exempting the `NROS_` spelling is therefore not a hole: the value it
    # carries is gated one name over. Were the re-export to disappear, the
    # `ZPICO_` name would leave the forwarded set and take its own coverage
    # check with it — which is the failure this list cannot catch, and is why
    # each entry names the line rather than asserting the shape.
    NROS_MAX_LIVELINESS
    NROS_MAX_PUBLISHERS
    NROS_MAX_QUERYABLES
    NROS_MAX_SUBSCRIBERS

    # issue 1407 — a PATH, not a tuning knob, and its readers do not spell it:
    # every descriptor consumer reaches it through
    # `nros_sizing_descriptor::from_build_env()` (`DESCRIPTOR_ENV`), which puts
    # the rebuild edge on the FILE (issue 0491). It is forwarded only on the
    # C/C++ west road, because only `nano_ros_entry()` writes a descriptor
    # there; a Rust west application (`rust_cargo_application()`) has no
    # `nano_ros_entry()`, so there is no file for this knob to name and no
    # Kconfig symbol a `$DOTCONFIG` rung could carry a path in.
    NROS_SIZING_DESCRIPTOR
)

# --- pure harvests, so the self-test can drive them on synthetic text --------

# ENV names of every forwarded knob, BOTH helpers. stdin: cmake text.
#
# issue 1505 — the optional `_derivable` segment is what this used to miss.
#
# A THIRD helper spelled `_nros_resolve_<x>_knob(` would be missed exactly as
# `_derivable_` was, so the alternation is not left to be trusted:
# `check_helper_spellings` below asserts that the set of helpers DEFINED in the
# cmake module is the set this pattern covers. The rule is "every forwarder is
# harvested", and without that assertion the reach is only ever as wide as
# whoever last edited this regex.
forwarded_knobs() {
    grep -oE '_nros_resolve(_derivable)?_knob\(([A-Z0-9_]+)' \
        | sed -E 's/^_nros_resolve(_derivable)?_knob\(//' | sort -u
}

# The helper names `forwarded_knobs` covers, and the ones the module DEFINES.
# stdin: cmake text. Prints any definition the harvest pattern cannot reach.
HARVESTED_HELPERS='_nros_resolve_knob _nros_resolve_derivable_knob'
check_helper_spellings() {
    local defined missed=""
    # `|| true`: "no definitions found" is this check's own negative control,
    # and grep reports it as exit 1 — which under `set -e` would kill the
    # script AT THE ASSIGNMENT, making the emptiness test below dead code that
    # reads as coverage (issue 1249). Same idiom the knob harvest uses.
    defined="$(grep -oE '^function\(_nros_resolve[A-Za-z0-9_]*_knob' <<<"$1" \
        | sed 's/^function(//' | sort -u || true)"
    [ -n "$defined" ] || { echo "no forwarder definitions found"; return 1; }
    local h
    while read -r h; do
        [ -n "$h" ] || continue
        case " $HARVESTED_HELPERS " in
            *" $h "*) ;;
            *) missed="$missed $h" ;;
        esac
    done <<<"$defined"
    [ -z "$missed" ] || { echo "$missed"; return 1; }
    return 0
}

# `<ENV> <CONFIG_SYM>` for every call whose VALUE is a bare `"${CONFIG_...}"`
# reference — the producer writing the pairing down. Both spellings of the
# helper, because this arm needs no reader to exist. stdin: cmake text.
declared_pairings() {
    tr '\n' ' ' \
        | grep -oE '_nros_resolve(_derivable)?_knob\([A-Z0-9_]+[[:space:]]+"\$\{CONFIG_[A-Z0-9_]+\}"' \
        | sed -E 's/^_nros_resolve(_derivable)?_knob\(//; s/[[:space:]]+"\$\{/ /; s/\}"$//' \
        | sort -u
}

# `<ENV> <CONFIG_SYM>` for every `KCONFIG_PAIRS` row. rustfmt splits a long pair
# across four lines, so flatten first. stdin: the resolver's source.
#
# SCOPED TO THE CONST, not grepped over the file. A `("X", "CONFIG_Y")` tuple in
# a doc example or a test vector is not a row, and reading one as a row is this
# gate's own failure mode one level in: it would report a pairing the resolver
# does not have, i.e. pass while the delivery is broken.
pairing_rows() {
    tr '\n' ' ' \
        | sed -E 's/.*const KCONFIG_PAIRS[^=]*=[[:space:]]*&\[//; s/\];.*//' \
        | grep -oE '"[A-Z0-9_]+"[[:space:]]*,[[:space:]]*"CONFIG_[A-Z0-9_]+"' \
        | sed -E 's/"//g; s/[[:space:]]*,[[:space:]]*/ /' \
        | sort -u
}

# The Kconfig symbol `Knob` will use for $1, given the rows in $2. This mirrors
# `nros_zephyr_build::kconfig_key_for` and must keep mirroring it.
reader_key_for() {
    local knob=$1 rows=$2 hit
    hit="$(awk -v k="$knob" '$1 == k { print $2; exit }' <<<"$rows")"
    if [ -n "$hit" ]; then printf '%s' "$hit"; else printf 'CONFIG_%s' "$knob"; fi
}

# Arm 2. $1: cmake text, $2: resolver text. Prints failures; returns 1 on any.
check_pairings() {
    local cmake_text=$1 resolver_text=$2
    local pairings rows bad=0 knob sym want
    pairings="$(declared_pairings <<<"$cmake_text")"
    rows="$(pairing_rows <<<"$resolver_text")"
    if [ -z "$pairings" ]; then
        echo "[FAIL] no '_nros_resolve_knob(<NAME> \"\${CONFIG_...}\")' pairings" >&2
        echo "       harvested — with none, this arm checks nothing while" >&2
        echo "       printing a number (issue 1490's shape, one level up)." >&2
        return 1
    fi
    while read -r knob sym; do
        [ -n "$knob" ] || continue
        want="$(reader_key_for "$knob" "$rows")"
        if [ "$want" != "$sym" ]; then
            echo "[FAIL] $CMAKE forwards $knob from $sym, and the Rust lane" >&2
            echo "       resolves it from $want." >&2
            echo "       On a Zephyr Rust image that reads the crate default" >&2
            echo "       whatever Kconfig says (issues 0460, 1490)." >&2
            echo "       Fix the row in $RESOLVER's KCONFIG_PAIRS:" >&2
            echo "         (\"$knob\", \"$sym\")," >&2
            bad=1
        fi
    done <<<"$pairings"
    return "$bad"
}

self_test() {
    # Negative controls for arm 2, on synthetic text. A pairing gate that
    # cannot fail is the thing issue 1490 found: green, specific and silent.
    local cmake_ok cmake_bad rows_ok rows_empty
    cmake_ok='_nros_resolve_knob(ZPICO_RING "${CONFIG_NROS_RING}")
_nros_resolve_derivable_knob(NROS_PLAIN "${CONFIG_NROS_PLAIN}")'
    rows_ok='const KCONFIG_PAIRS: &[(&str, &str)] = &[("ZPICO_RING", "CONFIG_NROS_RING")];'
    rows_empty='const KCONFIG_PAIRS: &[(&str, &str)] = &[];'

    check_pairings "$cmake_ok" "$rows_ok" 2>/dev/null \
        || { echo "[FAIL] selftest: a correct pairing was rejected" >&2; return 1; }
    # A knob whose two names are DIFFERENT WORDS and has no row: issue 1490's
    # exact defect, which the mention test passed over.
    if check_pairings "$cmake_ok" "$rows_empty" 2>/dev/null; then
        echo "[FAIL] selftest: a MISSING pairing row was accepted" >&2
        return 1
    fi
    # A row pointing at the wrong symbol — the failure a per-reader row test
    # could never see, because a row EXISTED.
    if check_pairings "$cmake_ok" \
        'const KCONFIG_PAIRS: &[(&str, &str)] = &[("ZPICO_RING", "CONFIG_NROS_WRONG")];' \
        2>/dev/null; then
        echo "[FAIL] selftest: a WRONG pairing row was accepted" >&2
        return 1
    fi
    # A derived-identical knob must need no row, or every reader is forced to
    # author 33 rows that state what the derivation already computes.
    check_pairings '_nros_resolve_knob(NROS_PLAIN "${CONFIG_NROS_PLAIN}")' "$rows_empty" \
        2>/dev/null \
        || { echo "[FAIL] selftest: a derived-identical knob demanded a row" >&2; return 1; }
    # A tuple OUTSIDE the const must not be read as a row. Otherwise the gate
    # reports a pairing the resolver does not have — passing while delivery is
    # broken, which is the one way a pairing gate can be worse than none.
    if check_pairings "$cmake_ok" \
        "$rows_empty"' fn t() { assert_eq!(f("ZPICO_RING"), "CONFIG_NROS_RING"); }' \
        2>/dev/null; then
        echo "[FAIL] selftest: a tuple outside KCONFIG_PAIRS was read as a row" >&2
        return 1
    fi
    # And the harvest itself must be able to come up empty loudly.
    cmake_bad='# nothing here forwards anything'
    if check_pairings "$cmake_bad" "$rows_ok" 2>/dev/null; then
        echo "[FAIL] selftest: an EMPTY cmake harvest was accepted" >&2
        return 1
    fi

    # --- issue 1505: arm 1's harvest reaches BOTH helpers ------------------
    #
    # Asserted on `forwarded_knobs` directly, because the miss it is about was
    # SILENT: the gate reported 47 knobs over a module forwarding 74 and said
    # nothing was wrong. A control that only ran the whole gate would have
    # passed throughout.
    local harvest
    harvest="$(printf '%s\n' \
        '_nros_resolve_knob(NROS_PLAIN "${CONFIG_NROS_PLAIN}")' \
        '_nros_resolve_derivable_knob(NROS_DERIV "${CONFIG_NROS_DERIV}")' \
        | forwarded_knobs | tr '\n' ' ')"
    if [ "$harvest" != "NROS_DERIV NROS_PLAIN " ]; then
        echo "[FAIL] selftest: the harvest missed a helper spelling — got '$harvest'" >&2
        echo '       _nros_resolve_knob( is not a substring of' >&2
        echo '       _nros_resolve_derivable_knob(, which is issue 1505.' >&2
        return 1
    fi
    # The positive control's twin: a harvest that read ONLY the plain spelling
    # must be visibly different, or the assertion above proves nothing.
    local narrow
    narrow="$(printf '%s\n' \
        '_nros_resolve_derivable_knob(NROS_DERIV "${CONFIG_NROS_DERIV}")' \
        | grep -oE '_nros_resolve_knob\(([A-Z0-9_]+)' | wc -l)"
    if [ "$narrow" != "0" ]; then
        echo "[FAIL] selftest: the OLD narrow pattern matched a derivable call," >&2
        echo "       so this control cannot distinguish the two harvests." >&2
        return 1
    fi
    # The helper-spelling assertion, both directions.
    if ! check_helper_spellings 'function(_nros_resolve_knob a b)
function(_nros_resolve_derivable_knob a b c)' >/dev/null; then
        echo "[FAIL] selftest: the two real helpers were reported as missed" >&2
        return 1
    fi
    if check_helper_spellings 'function(_nros_resolve_knob a b)
function(_nros_resolve_lazy_knob a b)' >/dev/null 2>&1; then
        echo "[FAIL] selftest: a THIRD forwarder spelling was accepted — the" >&2
        echo "       harvest would skip its knobs silently (issue 1505)." >&2
        return 1
    fi
    if check_helper_spellings 'nothing here defines a forwarder' >/dev/null 2>&1; then
        echo "[FAIL] selftest: NO forwarder definitions was accepted" >&2
        return 1
    fi
    return 0
}

[ -f "$CMAKE" ] || { echo "[FAIL] missing $CMAKE" >&2; exit 1; }
[ -f "$RESOLVER" ] || { echo "[FAIL] missing $RESOLVER" >&2; exit 1; }

self_test || exit 1

fail=0

# --- arm 2: the pairing the reader cannot know ------------------------------
check_pairings "$(cat "$CMAKE")" "$(cat "$RESOLVER")" || fail=1

# --- arm 1: coverage of the cmake list --------------------------------------
# `|| true`: "no calls found" is this gate's own negative control, and grep
# reports it as exit 1 — which under pipefail would kill the gate before it
# could say so, turning a loud [FAIL] into a bare status (issue 1249).
# issue 1505 — before trusting the harvest, prove it reaches every forwarder
# the module DEFINES. A regex is only as wide as its last edit.
if ! _missed="$(check_helper_spellings "$(cat "$CMAKE")")"; then
    echo "[FAIL] $CMAKE defines forwarder(s) the harvest cannot reach:$_missed" >&2
    echo '       forwarded_knobs() would silently skip every knob passed to' >&2
    echo "       them, which is issue 1505 exactly — that miss cost 27 of 74" >&2
    echo "       knobs and the gate reported success throughout." >&2
    echo "       Widen the alternation AND add the name to HARVESTED_HELPERS." >&2
    exit 1
fi

knobs="$(forwarded_knobs < "$CMAKE" || true)"
[ -n "$knobs" ] || { echo "[FAIL] no _nros_resolve_knob() calls found in $CMAKE" >&2; exit 1; }

checked=0
for knob in $knobs; do
    checked=$((checked + 1))
    found=0
    injected_paths=()
    for row in "${INJECTED_READERS[@]}"; do injected_paths+=("${row%%|*}"); done
    for f in "${READERS[@]}" "${injected_paths[@]}"; do
        [ -f "$f" ] || continue
        if nros_grep_q -F "\"$knob\"" "$f"; then
            found=1
            # issue 0751 — the name APPEARING is not the name being resolved
            # through `$DOTCONFIG`. A forwarded knob read with a bare
            # `env::var("<KNOB>")` yields the crate DEFAULT on a Zephyr Rust
            # image: issue 0460 itself, wearing the shape that satisfies this
            # gate's own test.
            #
            # Not hypothetical. That is what `nros-params/build.rs` did before
            # #0749's follow-up, and it was caught only because the file was not
            # yet listed as a reader — once listed, this arm passed over it.
            if nros_grep_q -F "env::var(\"$knob\")" "$f"; then
                echo "[FAIL] $f reads forwarded knob $knob with a bare env::var" >&2
                echo "       On a Zephyr Rust image that yields the crate default" >&2
                echo "       whatever Kconfig says (issue 0460). Resolve it with" >&2
                echo "       nros_zephyr_build::knob() instead." >&2
                fail=1
            fi
            break
        fi
    done
    if [ "$found" = 0 ]; then
        # A knob stated in a shared manifest. Unquoted, word-bounded.
        for f in "${MANIFEST_READERS[@]}"; do
            [ -f "$f" ] || continue
            if nros_grep_q -E "(^|[[:space:]])$knob([[:space:]]|\$)" "$f"; then
                found=1
                break
            fi
        done
    fi
    if [ "$found" = 0 ]; then
        for allowed in "${NO_RUST_READER[@]}"; do
            [ "$knob" = "$allowed" ] && { found=1; break; }
        done
    fi
    if [ "$found" = 0 ]; then
        echo "[FAIL] $knob is forwarded by $CMAKE but no Rust build script reads it" >&2
        fail=1
    fi
done

# --- arm 3: a listed reader must reach the ONE ladder -----------------------
# Every reader used to be held to `nros_zephyr_build::(knob_usize|dotconfig_usize)`,
# an alternation that could grow a third member and did. One spelling now.
for f in "${READERS[@]}"; do
    [ -f "$f" ] || { echo "[FAIL] READERS names missing $f" >&2; fail=1; continue; }
    nros_grep_q -F 'nros_zephyr_build::knob(' "$f" || {
        echo "[FAIL] $f is listed as a knob reader but never calls" >&2
        echo "       nros_zephyr_build::knob() — so nothing it names is actually" >&2
        echo "       resolved from \$DOTCONFIG (issues 0751, 0460)." >&2
        fail=1
    }
done

# An injected reader is only a reader if a listed READER hands it the ladder.
# Naming the file without that is a knob resolved by an accessor nobody supplied.
for row in "${INJECTED_READERS[@]}"; do
    f="${row%%|*}"; token="${row##*|}"
    [ -f "$f" ] || { echo "[FAIL] INJECTED_READERS names missing $f" >&2; fail=1; continue; }
    reached=0
    for r in "${READERS[@]}"; do
        [ -f "$r" ] || continue
        # COMMENTS STRIPPED — every reader also discusses this crate in prose.
        stripped="$(sed 's|//.*||' "$r")"
        nros_grep_q -F -- "$token" <<<"$stripped" && { reached=1; break; }
    done
    if [ "$reached" = 0 ]; then
        echo "[FAIL] $f is listed as an injected knob reader but no READER" >&2
        echo "       names \`$token\` — so nothing hands it the ladder and the" >&2
        echo "       knobs it states are resolved from the environment only." >&2
        fail=1
    fi
done

# A manifest reader is only a reader if a lane actually consumes it. Listing a
# file here and having nothing read it would be exactly the silence this gate
# exists to break, one indirection further out.
for f in "${MANIFEST_READERS[@]}"; do
    [ -f "$f" ] || { echo "[FAIL] MANIFEST_READERS names missing $f" >&2; fail=1; continue; }
    consumed=0
    for r in "${READERS[@]}"; do
        [ -f "$r" ] || continue
        # COMMENTS STRIPPED, and the full repo-relative path, not the basename.
        # Every one of these readers also NAMES the manifest in prose, so a
        # basename grep over the raw file passes on a reader that only talks
        # about it — the "is this coverage or is it a mention?" confusion this
        # gate is otherwise about. Here-string, never a pipe (issue 1077).
        stripped="$(sed 's|//.*||' "$r")"
        nros_grep_q -F -- "$f" <<<"$stripped" && { consumed=1; break; }
    done
    if [ "$consumed" = 0 ]; then
        echo "[FAIL] $f is listed as a knob manifest but no Rust reader reads it" >&2
        echo "       — a manifest nobody parses states knobs that reach nothing." >&2
        fail=1
    fi
done

if [ "$fail" != 0 ]; then
    echo "" >&2
    echo "  A Zephyr RUST image inherits none of cmake's set(ENV{...}) knob" >&2
    echo "  exports (issue 0460). Resolve the knob with" >&2
    echo "  nros_zephyr_build::knob(\"<ENV NAME>\"), and give it a" >&2
    echo "  KCONFIG_PAIRS row only if its Kconfig symbol is not CONFIG_<ENV NAME>." >&2
    exit 1
fi

pairings_checked="$(declared_pairings < "$CMAKE" | wc -l)"
echo "kconfig-knob-forwarding OK — $checked forwarded knob(s) each read by the Rust" \
     "lane, $pairings_checked cmake-declared pairing(s) matched against KCONFIG_PAIRS."
