#!/usr/bin/env bash
# The crate + feature set the PUBLISHED API docs are built from — issue 1110.
#
# ONE spelling, because there are two consumers and they must not drift:
#
#   just book                 builds and deploys these pages (`docs.yml`)
#   just check rustdoc-links  proves they still build, on a lane a PR runs
#
# The gate exists because nothing on a merge-gating path ran rustdoc, and
# rustdoc's broken-link lint is deny-level here. `3941b569a` deleted
# `ClientTrait::is_server_ready` while the paragraph beside it still linked to
# it, and `just book` — plus every docs deploy — stayed red for three days
# without any lane saying so. A second break (`nros-rmw-zenoh` naming a
# `record_alloc_ceilings` that never existed) was sitting behind it, invisible
# because rustdoc stops at the first crate that fails.
#
# The FEATURES are load-bearing and were learned the hard way (2026-08-21):
# without `std,env,macros` rustdoc drops `ExecutorConfigEnvExt::from_env`, the
# alloc-gated `ExecutorNodeRuntime::spin` and `nros::node!` — and every doc
# comment linking those then fails to resolve. A narrower set does not make the
# gate cheaper, it makes it wrong.
#
# Scope is the DEPLOYED set, not the workspace — and it STAYS that. The
# workspace pass is its own scope further down (`nros_rustdoc_workspace_*`,
# issue 1116 / phase-452 W4), with its own gate, because this list is what
# `just book` deploys and what the pull-request lane answers for. Widening
# THIS list would make a PR red for a crate the book does not publish.
#
# Issue 1116 recorded that wider pass as "~70 diagnostics, five crates that
# cannot document at all". Re-measured 2026-09-25 with this feature set it was
# 179 diagnostics and TWELVE such crates — three weeks of drift on a surface
# nothing checked. All 179 are fixed; the new gate holds the zero.
NROS_RUSTDOC_FEATURES="rmw-cffi,platform-posix,ros-humble,safety-e2e,std,env,macros"
NROS_RUSTDOC_CRATES=(
    nros
    nros-rmw
    nros-rmw-cffi
    nros-rmw-zenoh
    nros-platform-api
    nros-platform-cffi
)

# `-p a -p b …`, for a caller that wants the cargo spelling.
nros_rustdoc_package_args() {
    local p
    for p in "${NROS_RUSTDOC_CRATES[@]}"; do
        printf '%s\n' "-p" "$p"
    done
}

# ---------------------------------------------------------------------------
# The vendored sources this crate set's BUILD SCRIPTS require — issue 1138.
#
# `cargo doc` runs build scripts, so documenting a crate is a BUILD of
# everything under it. `nros-rmw-zenoh` declares `zpico-sys` as a NON-optional
# path dependency, and `nros-zpico-build`'s runner PANICS when
# `zenoh-pico/include` is absent (issue 0390). That panic is right for a build —
# no image exists without the source — and wrong as the verdict of a doc-LINK
# gate, which is asking a different question and is the thing that reports.
#
# NARROWING THE FEATURE SET CANNOT SIDESTEP IT, which is the alternative issue
# 1138 recorded as unmeasured. Measured here: the dependency is unconditional
# (`packages/rmw/zenoh/nros-rmw-zenoh/Cargo.toml`, `zpico-sys = { version =
# "0.5.0", path = "../zpico-sys", default-features = false }` — no `optional`),
# and the panic sits AHEAD of every backend branch in `runner.rs`, on the
# `backend_count == 0` path a plain `cargo doc` takes. A feature can drop what
# zpico-sys COMPILES; it can never drop the build script. The only way out
# would be dropping `nros-rmw-zenoh` from the documented set — the crate whose
# dangling `record_alloc_ceilings` link is half of why this gate exists.
#
# So the source is a genuine PRECONDITION, and the gate reports it as one
# instead of borrowing a build's panic. A wider SCOPE means revisiting this
# table: a new crate with a provisioning-dependent build script needs a row, or
# 1138 comes back under a different source name. Issue 1116 / phase-452 W4 did
# exactly that and its three extra rows are below, spliced onto these.
#
# Rows are `path|source-name|who-needs-it`.
NROS_RUSTDOC_SOURCE_REQS=(
    "packages/rmw/zenoh/zpico-sys/zenoh-pico/include|zenoh-pico|nros-rmw-zenoh -> zpico-sys build script (nros-zpico-build, issue 0390)"
)

# Prints one row per REQUIRED source that is absent in this checkout; prints
# nothing (and succeeds) when every one is present. The rows come in as
# ARGUMENTS so the two scopes below cannot grow two copies of the loop.
_nros_rustdoc_missing_from() {
    local row path
    for row in "$@"; do
        path="${row%%|*}"
        [ -e "$path" ] || printf '%s\n' "$row"
    done
}

nros_rustdoc_missing_sources() {
    _nros_rustdoc_missing_from "${NROS_RUSTDOC_SOURCE_REQS[@]}"
}

# ---------------------------------------------------------------------------
# THE WORKSPACE SCOPE — issue 1116 / phase-452 W4.
#
# The list above is the DEPLOYED set, and it stays that: `just book` reads it,
# and the pull-request `rustdoc-links` step exists to keep the docs deploy
# green. Widening it would make that lane answer for crates the book does not
# publish. So the workspace pass is a SECOND scope with its own gate
# (`just check rustdoc-workspace`), not a wider spelling of the first.
#
# MEMBERSHIP IS DERIVED, never authored: every workspace member except those
# declaring `[package.metadata.nros] embedded-only = true`, which is issue
# 1315's rule and the same derivation `HOST_UNCHECKABLE` uses. A hand list
# here would be the eight-crate string that issue retired, one lane over.
nros_rustdoc_workspace_scope_args() {
    local excludes
    if ! excludes="$(bash "$(dirname "${BASH_SOURCE[0]}")/embedded-only-members.sh")"; then
        echo "rustdoc-set: embedded-only-members.sh failed" >&2
        return 1
    fi
    printf '%s\n' "--workspace"
    # Word splitting is the point: the helper emits `--exclude <name>` pairs.
    # shellcheck disable=SC2086
    printf '%s\n' $excludes
}

# The sources the WORKSPACE pass needs. MEASURED 2026-09-25, by initialising
# submodules one at a time until `cargo doc --workspace` stopped dying inside a
# build script, and each row names the file that script actually reads:
# `cyclonedds-sys/build.rs` and `nros-rmw-xrce-cffi/build.rs` both read a
# vendored `CMakeLists.txt` and panic without it.
#
# It is a SUPERSET of the deployed table BY CONSTRUCTION — the deployed rows
# are spliced in rather than retyped, so the two scopes can never come to
# disagree about zenoh-pico.
#
# `mbedtls` and `px4-rs` are deliberately absent: the pass is green without
# them, and a row for a source nothing needs turns an ordinary checkout into a
# reported skip for no reason at all.
NROS_RUSTDOC_WORKSPACE_SOURCE_REQS=(
    "${NROS_RUSTDOC_SOURCE_REQS[@]}"
    "third-party/dds/cyclonedds/CMakeLists.txt|cyclonedds-src|cyclonedds-sys build script (issue 0390)"
    "packages/rmw/xrce/xrce-sys/micro-xrce-dds-client/CMakeLists.txt|micro-xrce-dds-client|nros-rmw-xrce-cffi build script (vendored_project_version)"
    "packages/rmw/xrce/xrce-sys/micro-cdr/CMakeLists.txt|micro-cdr|nros-rmw-xrce-cffi build script (vendored_project_version)"
)

nros_rustdoc_missing_workspace_sources() {
    _nros_rustdoc_missing_from "${NROS_RUSTDOC_WORKSPACE_SOURCE_REQS[@]}"
}
