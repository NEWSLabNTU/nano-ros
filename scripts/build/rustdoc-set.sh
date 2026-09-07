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
# Scope is the DEPLOYED set, not the workspace. A workspace-wide pass reports
# ~70 diagnostics in crates nothing publishes (issue 1116): real, worth fixing,
# and not what keeps the book from deploying. Widening this list is a ratchet
# decision, not a drive-by.
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
# instead of borrowing a build's panic. Widening NROS_RUSTDOC_CRATES (issue
# 1116) means revisiting this table: a new crate with a provisioning-dependent
# build script needs a row, or 1138 comes back under a different source name.
#
# Rows are `path|source-name|who-needs-it`.
NROS_RUSTDOC_SOURCE_REQS=(
    "packages/rmw/zenoh/zpico-sys/zenoh-pico/include|zenoh-pico|nros-rmw-zenoh -> zpico-sys build script (nros-zpico-build, issue 0390)"
)

# Prints one row per REQUIRED source that is absent in this checkout; prints
# nothing (and succeeds) when every one is present.
nros_rustdoc_missing_sources() {
    local row path
    for row in "${NROS_RUSTDOC_SOURCE_REQS[@]}"; do
        path="${row%%|*}"
        [ -e "$path" ] || printf '%s\n' "$row"
    done
}
