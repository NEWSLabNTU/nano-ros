//! Build script for `nros-rmw-xrce-cffi`.
//!
//! Compiles the K.2.0–K.2.4 C backend (`packages/rmw/xrce/nros-rmw-xrce/src/*.c`)
//! plus the vendored micro-XRCE-DDS-Client + micro-CDR sources directly
//! into a single static archive, then exposes the
//! `nros_rmw_xrce_register` symbol to the Rust side via `extern "C"`.
//!
//! Issue 1068 — this script OWNS NO SOURCE LIST. The set of C files compiled
//! here is read from `packages/rmw/xrce/xrce-sources.txt`, and
//! `nros-rmw-xrce/CMakeLists.txt` (the C/C++ consumers' entry point) reads the
//! same file. There used to be two hand-copied lists held together by a
//! comment; they drifted, and the drift was silent — `NROS_LINK_IP=0` dropped
//! `udp_transport{,_posix}.c` on this side and not on the CMake side, so a
//! serial-only XRCE node could not be built from C or C++ and the symptom was
//! a bigger image rather than an error. Adding a source file to only one lane
//! is now impossible: neither lane names one.
//!
//! phase-321 W1.d — the old comment named `packages/rmw/xrce/xrce-sys/build.rs`
//! as the mirror; that crate is deleted and only the DIRECTORY survives,
//! because it hosts the micro-XRCE-DDS-Client and micro-CDR submodules both
//! lanes compile from.

use std::{env, fs, path::PathBuf};

// NROS-XRCE-COMPILED-TREES-BEGIN
/// The `xrce-sources.txt` trees THIS lane compiles — phase-420 W9 step 4.
///
/// All three, and that is the point: the archive this script produces is the
/// one both lanes link, so it has to hold every TU. `nros-rmw-xrce/CMakeLists.txt`
/// declares the complementary set (`_xrce_compiled_trees`, `backend` only) and
/// LINKS this archive for the vendored halves instead of compiling them a second
/// time. Gate: `just check xrce-one-vendored-compile`.
///
/// Load-bearing, not a comment: the row loop asserts membership, so dropping a
/// tree here fails the build rather than silently shrinking the archive.
///
/// Why the split cannot go the other way — measured, phase-420 W9 step 4: the
/// backend TU `platform_aliases.c` DEFINES `uxr_millis`/`uxr_nanos`, which the
/// vendored `uxr` TUs call, while the backend TUs call the vendored session API.
/// The two halves are mutually recursive at link time, so they cannot be two
/// archives without `--start-group`, which rustc does not emit. One archive, or
/// a link that resolves by luck of ordering.
const COMPILED_TREES: &[&str] = &["uxr", "ucdr", "backend"];
// NROS-XRCE-COMPILED-TREES-END

/// The name of the pointer file this script writes into `OUT_DIR`.
///
/// `nros-rmw-xrce/CMakeLists.txt` links the archive cc-rs produces here, and
/// the archive's NAME and the generated headers' LOCATION are facts of this
/// script — so this script states them, in the one place a consumer can find
/// them from `OUT_DIR` alone. `just check rmw-xrce` locates `OUT_DIR` itself,
/// out of `cargo build --message-format=json`'s `build-script-executed`
/// message; it is NOT globbed, because `<hash>` is not predictable and taking
/// the first match of a glob is issue 0500's defect.
///
/// NOT `cargo::metadata=` — measured 2026-09-05: with no `links` key that
/// channel's `env` array comes back EMPTY in the JSON, and a `links` key buys
/// nothing here because the consumer is a CMake project, which can read no
/// `DEP_<LINKS>_*` at all.
const VENDOR_BUILD_POINTER: &str = "nros-xrce-vendor-build.txt";

// phase-420 W9 — this script OWNS NO CONFIGURATION VALUE either. The MTU
// defaults that used to live here as `XRCE_TRANSPORT_MTU_DEFAULT` /
// `XRCE_SERIAL_MTU_DEFAULT`, every other `@TOKEN@` the two upstream
// `config.h.in` templates take, every `#cmakedefine` toggle, and every knob's
// minimum are read from `packages/rmw/xrce/xrce-config.txt`, which
// `nros-rmw-xrce/CMakeLists.txt` reads too. Same remedy as the source list one
// file over (issue 1068): the values were restated in both lanes, and the
// lanes had already stopped agreeing.

fn main() {
    // phase-454 W6.b — the derivation's negative controls, on the NORMAL path.
    // A control nobody runs decays into a comment (`check-gate-selftests`
    // holds the same line for the Python gates), and this one guards a rule
    // whose failure is silent in the expensive direction: an image whose pool
    // grew because half the formula came from a declaration and half from a
    // literal still builds, boots and passes every knob gate.
    xrce_demand_selftest();

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // phase-321 W2.d — FOUR parents, not three. The crate sits at
    // packages/rmw/xrce/nros-rmw-xrce-cffi/, one level deeper than the old
    // packages/xrce/nros-rmw-xrce-cffi/. With three the walk stopped at
    // `packages/` and every vendored path came out doubled
    // (`<repo>/packages/packages/rmw/xrce/...`). A `.parent()` chain is a
    // relative path that no grep for "../" can find — only a build does.
    let workspace = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf();
    let xrce_sys = workspace.join("packages/rmw/xrce/xrce-sys");
    let xrce_c = workspace.join("packages/rmw/xrce/nros-rmw-xrce");
    let microcdr = xrce_sys.join("micro-cdr");
    let microxrce = xrce_sys.join("micro-xrce-dds-client");

    // Phase 145.4 — source-list drift / submodule-presence gate (mirrors the
    // zpico-sys 136.6 gate). The vendored uxr / micro-cdr C sources come from
    // git submodules; a missing checkout or an upstream bump that renamed a
    // source dir would otherwise surface as a confusing cc-rs "file not found"
    // mid-compile. Verify each vendored root resolves to a directory with .c
    // files (or subdirs) up front, with a clear init hint, and emit
    // rerun-if-changed so a submodule bump retriggers the build.
    for (label, root, hint) in [
        (
            "micro-xrce-dds-client",
            microxrce.join("src/c"),
            // #0390 — lead with the `nros setup` vocabulary a CLI-provisioned
            // user has; the label IS the `[source.*]` name. git line kept as the
            // underlying mechanism.
            "nros setup --source micro-xrce-dds-client  \
             (or: git submodule update --init packages/rmw/xrce/xrce-sys/micro-xrce-dds-client)",
        ),
        (
            "micro-cdr",
            microcdr.join("src/c"),
            "nros setup --source micro-cdr  \
             (or: git submodule update --init packages/rmw/xrce/xrce-sys/micro-cdr)",
        ),
        (
            "nros-rmw-xrce",
            xrce_c.join("src"),
            "in-repo wrapper — expected at packages/rmw/xrce/nros-rmw-xrce/src",
        ),
    ] {
        let has_sources = std::fs::read_dir(&root)
            .map(|entries| {
                entries.flatten().any(|e| {
                    e.path().extension().is_some_and(|x| x == "c")
                        || e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if !root.is_dir() || !has_sources {
            panic!(
                "nros-rmw-xrce-cffi: vendored `{label}` source root {} is missing or has no \
                 .c files — submodule not initialised or upstream layout drifted. Fix: {hint}",
                root.display()
            );
        }
        println!("cargo:rerun-if-changed={}", root.display());
    }

    // Issue 1069 — the two generated config headers take their version from
    // these files (`vendored_project_version`), so a submodule bump that moves
    // only the `project(… VERSION …)` line must still re-run this script. The
    // loop above watches `src/c`, which a version-only bump need not touch.
    for cml in [
        microxrce.join("CMakeLists.txt"),
        microcdr.join("CMakeLists.txt"),
    ] {
        println!("cargo:rerun-if-changed={}", cml.display());
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Phase 129.C.1 — platform fanout driven by `target_os` alone.
    // `nros-rmw-xrce-cffi` is platform-blind after 129.NET.3: the
    // session UDP path runs `xrce_nros_udp_init` on top of
    // `nros_platform_udp_*` regardless of platform. The build script
    // only still cares about `target_os` for two narrow reasons:
    //   1. Whether to compile the upstream `udp_transport*.c` and
    //      `util/time.c` POSIX-only TUs (they call libc directly).
    //   2. Whether to define `_POSIX_C_SOURCE` (needed to unlock
    //      `clock_gettime` / `getaddrinfo` in POSIX libc headers).
    // No `CARGO_FEATURE_PLATFORM_*` reads — the features that used
    // to gate these were deleted in 129.C.1.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let host_is_posix = matches!(
        target_os.as_str(),
        "linux" | "macos" | "freebsd" | "netbsd" | "openbsd"
    );
    // 129.C.1 — `transport_zephyr_udp` was superseded by `transport_nros_udp`,
    // so upstream's Zephyr platform is off everywhere. That used to be a
    // `let feat_zephyr = false;` here feeding a dead `else if` branch in the
    // header generator; phase-420 W9 moved it to `flag uxr
    // UCLIENT_PLATFORM_ZEPHYR never` in the shared manifest, where the CMake
    // lane makes the same claim from the same line.
    let is_posix = host_is_posix;
    let is_embedded = !host_is_posix;
    // Phase 204.7 — `NROS_LINK_IP=0` drops the IP (UDP/TCP) transport on a
    // serial-only hosted node (mirrors the zenoh `Z_FEATURE_LINK_*` gate). It gates
    // both the upstream `udp_transport*.c` sources and the `UCLIENT_PROFILE_UDP/TCP`
    // defines below. Embedded XRCE already excludes IP (custom transport), so this
    // only matters on POSIX. Default (unset) → IP on, unchanged.
    println!("cargo:rerun-if-env-changed=NROS_LINK_IP");
    let ip = !matches!(
        env::var("NROS_LINK_IP").ok().as_deref(),
        Some("0") | Some("false") | Some("off")
    );

    // Issue 1068 — the source list is DERIVED, not mirrored. Both this build
    // script and `nros-rmw-xrce/CMakeLists.txt` read
    // `packages/rmw/xrce/xrce-sources.txt`; neither holds a source path of its
    // own. See that file's header for the format and for why the conditions
    // live there rather than here.
    let manifest_path = workspace.join("packages/rmw/xrce/xrce-sources.txt");
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    let manifest = SourceManifest::read(&manifest_path);

    // phase-420 W9 — the same treatment for the VALUES. One statement of every
    // template token, every profile toggle and every knob, read by both lanes.
    let config_path = workspace.join("packages/rmw/xrce/xrce-config.txt");
    println!("cargo:rerun-if-changed={}", config_path.display());
    let config = ConfigManifest::read(&config_path);
    let knobs = KnobResolver::new();

    // NROS-XRCE-CONDITIONS-BEGIN — the boolean this lane supplies for each
    // condition token. `check-xrce-source-manifest` asserts the CMake lane
    // answers exactly this token set, and that it is exactly the set the two
    // manifests use; a manifest, not this match, decides what a token covers.
    //
    // ONE vocabulary for both manifests on purpose: `xrce-sources.txt` selects
    // FILES with these tokens and `xrce-config.txt` selects PROFILE DEFINES
    // with them, and those two answers have to agree — a header promising
    // `UCLIENT_PROFILE_UDP` whose `udp_transport.c` was not compiled is the
    // 1068 failure wearing a link error.
    let condition = |token: &str| -> bool {
        match token {
            "always" => true,
            // `util/time.c` calls `clock_gettime` / `nanosleep`;
            // `transport_posix_{udp,serial}.c` need <sys/socket.h> /
            // <termios.h>. Embedded targets supply their own time and
            // transport through the registry.
            "posix" => is_posix,
            // Phase 204.7 — `NROS_LINK_IP=0` sheds the IP link on a
            // serial-only node. Embedded XRCE already excludes IP.
            "posix_ip" => is_posix && ip,
            // phase-420 W9 — a toggle compiled off on every target. Stated
            // rather than omitted: an omitted `#cmakedefine` is `/* #undef */`
            // under `configure_file` and an UNTOUCHED `#cmakedefine` line
            // under a hand substitution, so silence is two different headers.
            "never" => false,
            other => panic!(
                "nros-rmw-xrce-cffi: an xrce manifest uses condition token `{other}`, which \
                 this build script does not answer. Add an arm here AND in \
                 nros-rmw-xrce/CMakeLists.txt — a token only one lane answers is issue 1068 \
                 again.",
            ),
        }
    };
    // NROS-XRCE-CONDITIONS-END

    // Generate config headers.
    generate_config(
        &out_dir,
        &microcdr.join("include/ucdr/config.h.in"),
        "include/ucdr/config.h",
        "ucdr",
        vendored_project_version(&microcdr.join("CMakeLists.txt"), "microcdr"),
        &config,
        &config_path,
        &knobs,
        &condition,
    );
    generate_config(
        &out_dir,
        &microxrce.join("include/uxr/client/config.h.in"),
        "include/uxr/client/config.h",
        "uxr",
        vendored_project_version(&microxrce.join("CMakeLists.txt"), "microxrcedds_client"),
        &config,
        &config_path,
        &knobs,
        &condition,
    );

    let mut build = cc::Build::new();
    // issue 0383 — implicit-function-declaration / int-conversion as errors
    // (`warnings(false)` only omits `-Wall`/`-Wextra`; cc-rs passes no `-w`).
    nros_cc_flags::strict_decls(&mut build);
    build
        .std("c99")
        .warnings(false)
        // Phase 204.9 — size: `-Os` + per-fn/data sections so the embedded
        // link path's `--gc-sections` (204.8) can strip unused XRCE surface.
        .opt_level_str("s")
        .flag_if_supported("-ffunction-sections")
        .flag_if_supported("-fdata-sections")
        .define("_DEFAULT_SOURCE", None)
        .include(out_dir.join("include"))
        .include(microcdr.join("include"))
        .include(microxrce.join("include"))
        .include(microxrce.join("src/c"))
        .include(xrce_c.join("src"))
        .include(xrce_c.join("include"))
        .include(workspace.join("packages/core/nros-rmw-abi/include"))
        .include(workspace.join("packages/platform/nros-platform-api/include"));
    if is_posix {
        // `_POSIX_C_SOURCE` is what unlocks `clock_gettime`,
        // `getaddrinfo`, etc in `<sys/socket.h>` + `<time.h>` on
        // glibc / musl / macOS. Bare-metal & Zephyr stdlibs don't
        // ship these — gating the define keeps the embedded build
        // from pulling in headers it can't satisfy.
        build.define("_POSIX_C_SOURCE", Some("200809L"));
    }

    let tree_root = |tree: &str| -> PathBuf {
        match tree {
            "uxr" => microxrce.join("src/c"),
            "ucdr" => microcdr.join("src/c"),
            "backend" => xrce_c.join("src"),
            other => panic!(
                "nros-rmw-xrce-cffi: {} names unknown source tree `{other}`",
                manifest_path.display()
            ),
        }
    };

    let mut compiled = 0usize;
    for row in &manifest.rows {
        assert!(
            COMPILED_TREES.contains(&row.tree.as_str()),
            "nros-rmw-xrce-cffi: {} has a row in tree `{}`, which COMPILED_TREES does not \
             list. This lane compiles EVERY tree — the archive it produces is the one \
             `nros-rmw-xrce/CMakeLists.txt` links, so a tree missing here is a tree missing \
             from that link. phase-420 W9 step 4.",
            manifest_path.display(),
            row.tree,
        );
        if condition(manifest.condition_of(&row.group, &manifest_path)) {
            build.file(tree_root(&row.tree).join(&row.path));
            compiled += 1;
        }
    }
    assert!(
        compiled > 0,
        "nros-rmw-xrce-cffi: {} selected no sources — manifest or condition drift",
        manifest_path.display()
    );

    if is_embedded {
        // Tell `<uxr/client/config_internal.h>` not to require the
        // POSIX TUs we've just dropped from the source list.
        build.define("UCLIENT_PLATFORM_NO_POSIX", None);
    }

    // phase-420 W9 — the backend's `-D` pool knobs, from the shared manifest's
    // `define` records. Phase 207.6 is why they exist: a pub-only bare-metal
    // node drops subscribers/services to 1, the ring to 1 and the buffer to
    // 256, and with `STREAM_HISTORY=4` plus a 512-byte MTU the session struct
    // falls from ~390 KB to ~10–20 KB.
    //
    // A `define` row states NO default — `nros-rmw-xrce/src/internal.h` holds
    // it in an `#ifndef`, and that is the one statement of it. Nothing stated
    // ⇒ nothing defined ⇒ the header's default stands, identically in both
    // lanes.
    //
    // phase-454 W6.b — rung 3.5 sits between them: what the image's own
    // declarations imply, read from the sizing descriptor (RFC-0100 D4/D5). A
    // stated value still wins, and an image with no descriptor reaches exactly
    // the bytes it did before this wave.
    let demand = XrceDemand::derive(sizing_descriptor().as_ref());
    demand.report();
    for row in &config.defines {
        let value = knobs
            .stated(&row.env, row.min, &config_path)
            .or_else(|| demand.for_knob(&row.env));
        if let Some(n) = value {
            build.define(&row.macro_name, n.to_string().as_str());
        }
    }

    let archive_stem = "nros_rmw_xrce_c_inline";
    build.compile(archive_stem);

    // phase-420 W9 step 4 — SAY WHERE THE ARCHIVE IS, in the one place a
    // consumer reaching `OUT_DIR` can read it.
    //
    // `nros-rmw-xrce/CMakeLists.txt` used to compile the vendored
    // micro-XRCE-DDS-Client and micro-CDR TUs a SECOND time, from the same
    // manifest with its own flags, so its CTest harness validated objects no
    // image contains. It now links THIS archive. Two facts have to cross that
    // seam — what cc-rs called the archive, and where `generate_config` put the
    // headers those objects were compiled against — and both are facts of this
    // file, so this file states them rather than the consumer guessing.
    //
    // A `#`-commented `key=value` file, the same dependency-free shape as
    // `xrce-sources.txt` / `xrce-config.txt`, so the reader needs no parser.
    let archive = out_dir.join(format!("lib{archive_stem}.a"));
    assert!(
        archive.is_file(),
        "nros-rmw-xrce-cffi: cc-rs did not leave `{}` where this script expects it. \
         `nros-rmw-xrce/CMakeLists.txt` links that path.",
        archive.display()
    );
    let pointer = out_dir.join(VENDOR_BUILD_POINTER);
    fs::write(
        &pointer,
        format!(
            "# Written by nros-rmw-xrce-cffi/build.rs — phase-420 W9 step 4.\n\
             # Read by nros-rmw-xrce/CMakeLists.txt, which LINKS this archive instead of\n\
             # compiling the vendored micro-XRCE-DDS-Client / micro-CDR sources again.\n\
             # Do not hand-edit: it is regenerated on every build of this crate.\n\
             archive={}\n\
             include={}\n",
            archive.display(),
            out_dir.join("include").display(),
        ),
    )
    .unwrap_or_else(|e| {
        panic!(
            "nros-rmw-xrce-cffi: cannot write {} ({e})",
            pointer.display()
        )
    });

    // Phase 129.NET.3 — `transport_nros_udp.c` references the
    // canonical `nros_platform_udp_*` ABI. Ship the sibling
    // `nros-platform-posix` C port inside this crate's static
    // archive whenever the build resolves to a POSIX host
    // (explicit `platform-posix` feature or host-OS auto-detect).
    // Consumers that bring their own platform-provider library
    // (e.g. the C SDK linked under cmake with `nano_ros_link_platform`)
    // must opt out by NOT selecting `posix` / `platform-posix`
    // and forcing a non-host target — otherwise the link hits
    // duplicate-symbol errors.
    if is_posix {
        let posix_src = workspace.join("packages/platform/nros-platform-posix/src");
        let mut posix_build = cc::Build::new();
        // issue 0383 — implicit-function-declaration / int-conversion as errors.
        nros_cc_flags::strict_decls(&mut posix_build);
        posix_build
            .std("c11")
            .warnings(false)
            .define("_DEFAULT_SOURCE", None)
            .define("_POSIX_C_SOURCE", Some("200809L"))
            .include(workspace.join("packages/platform/nros-platform-api/include"))
            .file(posix_src.join("platform.c"))
            .file(posix_src.join("net.c"))
            .file(posix_src.join("timer.c"));
        posix_build.compile("nros_platform_posix_link");
        println!("cargo:rerun-if-changed={}", posix_src.display());
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", xrce_c.join("src").display());
    println!(
        "cargo:rerun-if-changed={}",
        xrce_c.join("include").display()
    );
}

// NROS-XRCE-VERSIONS-BEGIN
/// The vendored tree's OWN statement of its version — issue 1069.
///
/// `MICROCDR_VERSION_STR` compiled as `"2.0.2"` here and `"2.4.1"` under the
/// CMake lane, for one tree that is 2.0.2: the versions were hand-restated in
/// four places (both lanes, plus two `nros-sdk-index.toml` rows) and disagreed.
/// Correcting the literals would have left the same four literals, so both
/// lanes read the fact instead — each vendored `CMakeLists.txt` says
/// `project(<name> VERSION "X.Y.Z")`, which is upstream telling us what the
/// tree is. `nros-rmw-xrce/CMakeLists.txt` has the CMake twin,
/// `_nros_xrce_project_version()`. Gate: `just check xrce-vendored-versions`.
///
/// NOT `git describe --tags`, which issue 1069 proposed: a submodule is fetched
/// by SHA with no tags, so `git -C …/micro-xrce-dds-client describe --tags`
/// answers "No tags can describe" on an ordinary checkout.
///
/// Panics rather than defaulting: a wrong version compiles into a public macro
/// and past upstream's own `#if UXR_CLIENT_VERSION_MAJOR >= 4` tripwire, so
/// "close enough" is exactly the failure mode being retired.
fn vendored_project_version(cmakelists: &std::path::Path, project: &str) -> [String; 3] {
    let text = fs::read_to_string(cmakelists).unwrap_or_else(|e| {
        panic!(
            "nros-rmw-xrce-cffi: cannot read {} ({e}) — the vendored {project} checkout \
             is missing. Run `nros setup --source micro-cdr --source \
             micro-xrce-dds-client` (or `git submodule update --init`).",
            cmakelists.display()
        )
    });
    let mut found: Vec<[String; 3]> = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some(rest) = line.strip_prefix("project(") else {
            continue;
        };
        let mut tok = rest.split_whitespace();
        if tok.next() != Some(project) {
            continue;
        }
        if !tok
            .next()
            .is_some_and(|k| k.eq_ignore_ascii_case("VERSION"))
        {
            continue;
        }
        let raw = tok.next().unwrap_or("").trim_matches('"');
        let parts: Vec<&str> = raw.split('.').collect();
        if parts.len() != 3
            || !parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        {
            panic!(
                "nros-rmw-xrce-cffi: `project({project} VERSION …)` in {} does not state \
                 an X.Y.Z version (got `{raw}`).",
                cmakelists.display()
            );
        }
        found.push([
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
        ]);
    }
    match found.len() {
        1 => found.pop().unwrap(),
        n => panic!(
            "nros-rmw-xrce-cffi: expected exactly one `project({project} VERSION …)` line in \
             {}, found {n}. Upstream changed how it states its version — update BOTH lanes \
             (this function and `_nros_xrce_project_version()` in \
             nros-rmw-xrce/CMakeLists.txt) together.",
            cmakelists.display()
        ),
    }
}
// NROS-XRCE-VERSIONS-END

/// Fill in one upstream `config.h.in` from the shared manifest — phase-420 W9.
///
/// ONE implementation for both templates, and the CMake lane's `configure_file`
/// calls are the twin. Before this there were four: a hand-written
/// `generate_ucdr_config` and `generate_uxr_config` here, and two
/// `configure_file`s there, each restating the token values. The two lanes
/// already disagreed about the version (issue 1069) for exactly that reason.
///
/// `@PROJECT_VERSION*@` is the one thing NOT read from the manifest: it comes
/// from the vendored tree's own `project(<name> VERSION …)` line, because a
/// version stated in our manifest would be one more hand-written restatement
/// of a fact upstream already holds.
#[allow(clippy::too_many_arguments)]
fn generate_config(
    out_dir: &std::path::Path,
    template_path: &std::path::Path,
    out_rel: &str,
    template: &str,
    version: [String; 3],
    config: &ConfigManifest,
    config_path: &std::path::Path,
    knobs: &KnobResolver,
    condition: &dyn Fn(&str) -> bool,
) {
    let text = fs::read_to_string(template_path).unwrap_or_else(|e| {
        panic!(
            "nros-rmw-xrce-cffi: cannot read the upstream template {} ({e}) — the vendored \
             checkout is missing. Run `nros setup --source micro-cdr --source \
             micro-xrce-dds-client` (or `git submodule update --init`).",
            template_path.display()
        )
    });
    println!("cargo:rerun-if-changed={}", template_path.display());

    let [maj, min, pat] = version;
    let mut h = text
        .replace("@PROJECT_VERSION_MAJOR@", &maj)
        .replace("@PROJECT_VERSION_MINOR@", &min)
        .replace("@PROJECT_VERSION_PATCH@", &pat)
        .replace("@PROJECT_VERSION@", &format!("{maj}.{min}.{pat}"));

    // `value` — a fixed substitution.
    for row in config.values_for(template) {
        h = h.replace(&format!("@{}@", row.token), &row.literal);
    }

    // `knob` — a substitution someone may choose. The ladder and the minimum
    // both live in the manifest, so the CMake lane enforces the same floor on
    // the same input.
    for row in config.knobs_for(template) {
        let v = knobs.value(&row.env, row.default, row.min, config_path);
        h = h.replace(&format!("@{}@", row.token), &v.to_string());
    }

    // `flag` — a `#cmakedefine` toggle. CMake writes `#define NAME` when the
    // variable is truthy and `/* #undef NAME */` when it is not; match that
    // byte for byte, because these two headers are diffed as an acceptance
    // criterion and a stylistic difference would read as a real one.
    //
    // Match the whole LINE (`\n` boundary) so the `UCLIENT_PLATFORM_POSIX`
    // rule does not also fire on `UCLIENT_PLATFORM_POSIX_NOPOLL`.
    for row in config.flags_for(template) {
        let on = condition(&row.condition);
        let line = if on {
            format!("#define {}\n", row.token)
        } else {
            format!("/* #undef {} */\n", row.token)
        };
        h = h.replace(&format!("#cmakedefine {}\n", row.token), &line);
    }

    let out = out_dir.join(out_rel);
    fs::create_dir_all(out.parent().expect("config.h has a parent dir")).unwrap();
    fs::write(&out, h).unwrap();
}

/// The knob ladder — ONE implementation, shared by the template `knob` rows and
/// the backend `define` rows. phase-420 W9.
///
/// Rungs, highest first:
///
///   1. the environment variable    — a person, right now
///   2. `CONFIG_<env>` in `$DOTCONFIG` — a person, in the tree (Kconfig)
///   3. the `[knobs.xrce]` rung     — this lane only; see below
///   4. the manifest's `<default>`  — nobody stated one ([`Self::value`] only)
///
/// issue 0460 — rung 2 is why this is not a bare `env::var`. Every one of these
/// knobs is forwarded by `_nros_resolve_knob()` in
/// `zephyr/cmake/nros_cargo_build.cmake` with `set(ENV{...})`, which reaches
/// the C lane's re-baked command and NOT the Rust lane's: zephyr-lang-rust's
/// `rust_cargo_application` builds its own cargo invocation and inherits
/// nothing, so a Zephyr Rust image read every one of them as unset whatever
/// Kconfig said. The env name is the Kconfig name minus `CONFIG_`, so the pair
/// is DERIVED rather than tabulated; `check-kconfig-knob-forwarding` proves the
/// cmake list and the readers agree.
///
/// Rung 3 is the one asymmetry between the lanes, and it is stated in
/// `xrce-config.txt` rather than left for a reader to discover: reading a
/// `[knobs.xrce]` TOML needs a parser the CMake lane does not have, it covers
/// exactly two knobs, and it can only be delivered by a cargo build — so it
/// cannot fire in a lane with no cargo.
struct KnobResolver {
    rungs: nros_platform_config::platform_config::XrceKnobs,
}

impl KnobResolver {
    fn new() -> Self {
        // phase-400 W6 — the `[knobs.xrce]` rungs sit under the env front-end
        // and above the manifest defaults.
        Self {
            rungs: nros_platform_config::platform_config::BuildRungs::from_build_env()
                .map(|r| r.xrce_rungs())
                .unwrap_or_default(),
        }
    }

    fn rung(&self, env_name: &str) -> Option<usize> {
        match env_name {
            "NROS_XRCE_CUSTOM_TRANSPORT_MTU" => self.rungs.custom_transport_mtu,
            "NROS_XRCE_STREAM_HISTORY" => self.rungs.stream_history,
            _ => None,
        }
    }

    /// Rungs 1–3: what a person or a platform config STATED, or `None`.
    ///
    /// An environment value that is not a number PANICS — someone typed it in
    /// this shell and falling through to a default would answer a question they
    /// did not ask. A Kconfig value that is not a `usize` is ABSENT instead:
    /// `-1` is the documented DERIVE sentinel (phase-403 W8), and a knob left
    /// on it means "nothing stated", not "malformed".
    fn stated(&self, env_name: &str, min: usize, config_path: &std::path::Path) -> Option<usize> {
        println!("cargo:rerun-if-env-changed={env_name}");
        let stated = match env::var(env_name) {
            Ok(raw) if !raw.is_empty() => Some(raw.parse::<usize>().unwrap_or_else(|_| {
                panic!("nros-rmw-xrce-cffi: {env_name}='{raw}' is not a number")
            })),
            _ => nros_zephyr_build::dotconfig_usize(&format!("CONFIG_{env_name}"))
                .or_else(|| self.rung(env_name)),
        };
        if let Some(n) = stated {
            if n < min {
                panic!(
                    "nros-rmw-xrce-cffi: {env_name}={n} is below the minimum {min} stated in {}",
                    config_path.display()
                );
            }
        }
        stated
    }

    /// Rungs 1–4: the value to compile with.
    fn value(
        &self,
        env_name: &str,
        default: usize,
        min: usize,
        config_path: &std::path::Path,
    ) -> usize {
        self.stated(env_name, min, config_path).unwrap_or(default)
    }
}

/// One `src <group> <tree> <path>` record from the shared source manifest.
struct SourceRow {
    group: String,
    tree: String,
    path: String,
}

/// `packages/rmw/xrce/xrce-sources.txt`, parsed — issue 1068.
///
/// The list of C files this backend compiles used to exist TWICE: here and in
/// `nros-rmw-xrce/CMakeLists.txt`, with a comment asserting they stayed in
/// lockstep. They did not — `NROS_LINK_IP=0` dropped the UDP transports on this
/// side and not on the CMake side, so a serial-only XRCE node could not be
/// built from C or C++. The list is now read from one file by both lanes.
///
/// The format is deliberately line-oriented and dependency-free: CMake reads
/// the same file with `file(STRINGS)`, and `cargo` is `--locked`-shimmed here,
/// so a TOML crate is not available to a build script that must also work from
/// a bare clone.
struct SourceManifest {
    /// group name → condition token, in declaration order.
    groups: Vec<(String, String)>,
    rows: Vec<SourceRow>,
}

impl SourceManifest {
    fn read(path: &std::path::Path) -> Self {
        let text = fs::read_to_string(path).unwrap_or_else(|e| {
            panic!(
                "nros-rmw-xrce-cffi: cannot read the shared XRCE source manifest {} ({e}). It is \
                 tracked in-repo; a missing one means the checkout is incomplete.",
                path.display()
            )
        });
        let mut groups: Vec<(String, String)> = Vec::new();
        let mut rows = Vec::new();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let f: Vec<&str> = line.split_whitespace().collect();
            match f.as_slice() {
                ["group", name, cond] => {
                    if groups.iter().any(|(g, _)| g == name) {
                        panic!(
                            "{}:{}: group `{name}` declared twice",
                            path.display(),
                            n + 1
                        );
                    }
                    groups.push(((*name).to_string(), (*cond).to_string()));
                }
                ["src", group, tree, rel] => rows.push(SourceRow {
                    group: (*group).to_string(),
                    tree: (*tree).to_string(),
                    path: (*rel).to_string(),
                }),
                _ => panic!(
                    "{}:{}: expected `group <name> <condition>` or `src <group> <tree> <path>`, \
                     got `{line}`",
                    path.display(),
                    n + 1
                ),
            }
        }
        Self { groups, rows }
    }

    fn condition_of<'a>(&'a self, group: &str, path: &std::path::Path) -> &'a str {
        self.groups
            .iter()
            .find(|(g, _)| g == group)
            .map(|(_, c)| c.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "{}: source names group `{group}`, which no `group` line declares",
                    path.display()
                )
            })
    }
}

/// `value <template> <token> <literal>` — a fixed `@token@` substitution.
struct ValueRow {
    template: String,
    token: String,
    literal: String,
}

/// `knob <template> <token> <env> <default> <min>` — a tunable substitution.
struct KnobRow {
    template: String,
    token: String,
    env: String,
    default: usize,
    min: usize,
}

/// `flag <template> <token> <condition>` — a `#cmakedefine` toggle.
struct FlagRow {
    template: String,
    token: String,
    condition: String,
}

/// `define <macro> <env> <min>` — a `-D` on the backend compile. No default
/// column by design: `nros-rmw-xrce/src/internal.h` holds it in an `#ifndef`.
struct DefineRow {
    macro_name: String,
    env: String,
    min: usize,
}

/// `packages/rmw/xrce/xrce-config.txt`, parsed — phase-420 W9.
///
/// Sibling of [`SourceManifest`]: that one answers "which files", this one
/// "with what values". Both lanes read both, and for the same reason — the
/// values used to exist TWICE (here, and as `set(UCLIENT_…)` plus
/// `configure_file` in `nros-rmw-xrce/CMakeLists.txt`) with nothing but
/// proximity holding them together. See the file's own header for the format,
/// the knob ladder, and the one rung this lane has that the other cannot.
struct ConfigManifest {
    values: Vec<ValueRow>,
    knobs: Vec<KnobRow>,
    flags: Vec<FlagRow>,
    defines: Vec<DefineRow>,
}

impl ConfigManifest {
    fn read(path: &std::path::Path) -> Self {
        let text = fs::read_to_string(path).unwrap_or_else(|e| {
            panic!(
                "nros-rmw-xrce-cffi: cannot read the shared XRCE config manifest {} ({e}). It is \
                 tracked in-repo; a missing one means the checkout is incomplete.",
                path.display()
            )
        });
        let mut m = Self {
            values: Vec::new(),
            knobs: Vec::new(),
            flags: Vec::new(),
            defines: Vec::new(),
        };
        let num = |field: &str, n: usize| -> usize {
            field.parse().unwrap_or_else(|_| {
                panic!("{}:{}: `{field}` is not a number", path.display(), n + 1)
            })
        };
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let f: Vec<&str> = line.split_whitespace().collect();
            match f.as_slice() {
                ["value", template, token, literal] => m.values.push(ValueRow {
                    template: (*template).to_string(),
                    token: (*token).to_string(),
                    literal: (*literal).to_string(),
                }),
                ["knob", template, token, env, default, min] => m.knobs.push(KnobRow {
                    template: (*template).to_string(),
                    token: (*token).to_string(),
                    env: (*env).to_string(),
                    default: num(default, n),
                    min: num(min, n),
                }),
                ["flag", template, token, condition] => m.flags.push(FlagRow {
                    template: (*template).to_string(),
                    token: (*token).to_string(),
                    condition: (*condition).to_string(),
                }),
                ["define", macro_name, env, min] => m.defines.push(DefineRow {
                    macro_name: (*macro_name).to_string(),
                    env: (*env).to_string(),
                    min: num(min, n),
                }),
                _ => panic!(
                    "{}:{}: expected `value <template> <token> <literal>`, \
                     `knob <template> <token> <env> <default> <min>`, \
                     `flag <template> <token> <condition>` or \
                     `define <macro> <env> <min>`, got `{line}`",
                    path.display(),
                    n + 1
                ),
            }
        }
        if m.values.is_empty() && m.knobs.is_empty() && m.flags.is_empty() {
            panic!(
                "{}: no substitutions at all — manifest drift",
                path.display()
            );
        }
        m
    }

    fn values_for(&self, template: &str) -> impl Iterator<Item = &ValueRow> {
        self.values.iter().filter(move |r| r.template == template)
    }

    fn knobs_for(&self, template: &str) -> impl Iterator<Item = &KnobRow> {
        self.knobs.iter().filter(move |r| r.template == template)
    }

    fn flags_for(&self, template: &str) -> impl Iterator<Item = &FlagRow> {
        self.flags.iter().filter(move |r| r.template == template)
    }
}

// ---------------------------------------------------------------------------
// phase-454 W6.b — what this image's own declarations imply for the XRCE pools.
//
// RFC-0100 D5: "the backend owns its formula, in its own build". So the
// arithmetic is here, beside the `cc::Build` that applies it, rather than in the
// shared reader — adding a fifth backend must touch no shared code, and a
// maintainer of this backend must not have to read another crate to find out
// how its buffers are sized.
//
// D2 gives the shape, one line per pool:
//
//   pool_bytes = COUNT(class) x SLOTS(class) x SLOT_BYTES(class) + fixed
//
//   reliable stream buffers  COUNT 1 per session  SLOTS history   SLOT_BYTES MTU
//   subscriber slots         COUNT subscriptions  SLOTS ring depth SLOT_BYTES bound
//
// The COUNT terms are already derived, on a road that predates this one
// (`NROS_XRCE_MAX_SUBSCRIBERS` and its two siblings, issue 1033, where ZERO is
// the answer and worth 33,296 bytes a slot). What this adds is the other two
// factors.
// ---------------------------------------------------------------------------

use nros_sizing_descriptor::{Basis, Endpoint, EndpointKind, Reliability, SizingDescriptor};
use std::collections::BTreeMap;

// The 4-byte CDR header `internal.h` names, and `xrce_stage_inbound` refuses a
// payload unless `len + header <= cap` — so a buffer sized at the bound alone
// is four bytes short of every message that actually reaches the bound.
const CDR_HEADER_LEN: usize = 4;

// The reliable streams' history when no endpoint declares reliable delivery.
//
// Not zero, and the reason is in `internal.h` beside the knob: both reliable
// streams stay LIVE on a best-effort-only image, because every control message
// this backend sends rides the output reliable stream and `uxr_buffer_request_data`
// names the input reliable stream as the Agent's delivery stream for every
// reader whatever its QoS. A declaration about DATA delivery is not a licence to
// make session setup lossy. Four is the protocol floor the header's `#error`
// states (reliable retransmit headroom), and upstream splits the buffer into
// `history` blocks, so an MTU-sized message still fits at 4.
const RELIABLE_CONTROL_HISTORY: usize = 4;

// Both buffers, at the default history and the default MTU — the number a
// declaration is worth, quoted in the refusal that says so.
const RELIABLE_STREAM_SAVING_BYTES: usize = 2 * (16 - RELIABLE_CONTROL_HISTORY) * 4096;

const ENV_STREAM_HISTORY: &str = "NROS_XRCE_STREAM_HISTORY";
const ENV_SUBSCRIBER_BUFFER: &str = "NROS_XRCE_SUBSCRIBER_BUFFER_SIZE";
const ENV_SUBSCRIBER_RING_DEPTH: &str = "NROS_XRCE_SUBSCRIBER_RING_DEPTH";

/// Rung 3.5 of the ladder in `packages/rmw/xrce/xrce-config.txt`.
///
/// Keyed on the ENV name a `define` row carries, never on the C macro: the
/// macro is a configuration value and `check-xrce-config-manifest` refuses a
/// lane that states one of its own (phase-420 W9). The env name is the row's
/// own field, so this is a lookup into the manifest rather than a second copy
/// of it.
#[derive(Default)]
struct XrceDemand {
    values: BTreeMap<&'static str, usize>,
    /// What was NOT derived, and why. D6: *"always the safe direction and
    /// always loud — the build prints what declaring would save"*.
    notes: Vec<String>,
}

impl XrceDemand {
    /// The value this image's declarations imply for `env`, if any.
    fn for_knob(&self, env: &str) -> Option<usize> {
        self.values.get(env).copied()
    }

    fn note(&mut self, what: impl Into<String>) {
        self.notes.push(what.into());
    }

    fn report(&self) {
        for n in &self.notes {
            println!("cargo::warning=nros-rmw-xrce: {n}");
        }
    }

    fn derive(desc: Option<&SizingDescriptor>) -> Self {
        let mut d = Self::default();
        // No descriptor: nobody ran `nros sync` for this image. Every number
        // below stays exactly where it was before this wave — the acceptance
        // criterion, not a convenience.
        let Some(desc) = desc else {
            return d;
        };

        // A `closure` basis describes every type the link closure can reach,
        // not the set this image creates. D6 forbids silently widening it, and
        // it cuts both ways here: sizing a POOL from the closure over-states,
        // while sizing a QoS-gated buffer from it can UNDER-state, because a
        // closure row carries no endpoint's QoS.
        if desc.meta.basis != Basis::Contract {
            d.note(format!(
                "the sizing descriptor states `basis = {}`, which describes the link closure \
                 rather than what this image creates, so no pool is derived from it \
                 (RFC-0100 D6)",
                desc.meta.basis
            ));
            return d;
        }
        // "Absence is not zero" — D6's second trigger. An endpoint that stated
        // no QoS at all is one whose reliability and depth this table cannot
        // speak for, and both derivations below are ALL-endpoint predicates.
        match desc.meta.undeclared_endpoints().get() {
            Some(0) => {}
            Some(n) => {
                d.note(format!(
                    "{n} endpoint(s) declare no QoS at all, so this image's reliability and \
                     depth are not attributable and the XRCE pools keep their defaults. \
                     Declaring them is worth up to {RELIABLE_STREAM_SAVING_BYTES} bytes of \
                     session heap"
                ));
                return d;
            }
            None => {
                d.note(
                    "the sizing descriptor refuses `undeclared_endpoints`, so it cannot say \
                     whether an endpoint stayed silent; the XRCE pools keep their defaults",
                );
                return d;
            }
        }
        if desc.endpoints.is_empty() {
            return d;
        }

        d.derive_stream_history(desc);
        d.derive_subscriber_family(desc);
        d
    }

    /// Reliability gates the two reliable stream buffers.
    ///
    /// The predicate is ALL, and "undeclared implies reliable" is what makes it
    /// the safe direction: an endpoint whose reliability nobody stated keeps
    /// the full history, so the saving needs a declaration rather than a
    /// silence.
    ///
    /// Services and actions count as reliable without asking. A ROS service is
    /// reliable by construction and an action expands into services — so a row
    /// of either kind means this stream carries traffic that must be
    /// retransmitted, which is exactly what phase 130.4's default of 16 was
    /// measured for (feedback + result + status_array + replies fanned out
    /// inside one user handler).
    fn derive_stream_history(&mut self, desc: &SizingDescriptor) {
        let is_best_effort = |e: &Endpoint| {
            matches!(e.kind, EndpointKind::Publisher | EndpointKind::Subscription)
                && e.reliability().stated() == Some(&Reliability::BestEffort)
        };
        if desc.endpoints.iter().all(is_best_effort) {
            self.values
                .insert(ENV_STREAM_HISTORY, RELIABLE_CONTROL_HISTORY);
            return;
        }
        // One declaration away from the saving: every endpoint that DID state a
        // reliability said best_effort, and the rest said nothing. Worth a line,
        // because this is the largest single number in the backend and nothing
        // else would tell them.
        let stated_any = desc.endpoints.iter().any(|e| e.reliability().is_stated());
        let none_reliable = desc
            .endpoints
            .iter()
            .all(|e| e.reliability().stated() != Some(&Reliability::Reliable));
        let pubsub_only = desc
            .endpoints
            .iter()
            .all(|e| matches!(e.kind, EndpointKind::Publisher | EndpointKind::Subscription));
        if stated_any && none_reliable && pubsub_only {
            self.note(format!(
                "no endpoint declares `reliability = reliable` and some declare nothing, so \
                 both reliable stream buffers keep the full history — an undeclared \
                 reliability is assumed reliable (RFC-0100 D6). Declaring `best_effort` on \
                 every endpoint is worth {RELIABLE_STREAM_SAVING_BYTES} bytes of session heap"
            ));
        }
    }

    /// The subscriber pool's two remaining factors, TOGETHER or not at all.
    ///
    /// The coupling is the whole reason this cannot make an image bigger. The
    /// pool is one product (D2) and the two factors move in opposite directions
    /// on a typical image: a declared `std_msgs/msg/String` prices its entry at
    /// 1,174 bytes against the literal 1,024, while a declared `depth = 10`
    /// prices the ring at 10 entries against the literal 32. Taking the first
    /// without the second is a 15% GROWTH; taking both is a 64% saving. A pool
    /// sized from one declaration and one literal describes no image at all.
    ///
    /// Refused outright when the image declares an action of either role: an
    /// action client opens a subscription underneath itself that no
    /// `[[endpoint]]` row itemises, so the maximum over the declared
    /// subscriptions would be short for it — the UNDER direction, which is the
    /// one that ships a runtime failure (issue 1319's lesson, one pool over).
    fn derive_subscriber_family(&mut self, desc: &SizingDescriptor) {
        let subs: Vec<&Endpoint> = desc
            .endpoints
            .iter()
            .filter(|e| e.kind == EndpointKind::Subscription)
            .collect();
        // No subscriptions: no demand. The slot COUNT is already derived to 0
        // on the other road, so the array is gone and its entry size prices
        // nothing. D7 — an abstention, not a floor.
        if subs.is_empty() {
            return;
        }
        if desc.endpoints.iter().any(|e| {
            matches!(
                e.kind,
                EndpointKind::ActionServer | EndpointKind::ActionClient
            )
        }) {
            self.note(
                "this image declares an action, which opens subscriptions the endpoint table \
                 does not itemise, so the subscriber ring keeps its default size and depth \
                 rather than being sized from the declared subscriptions alone",
            );
            return;
        }

        let mut bound = 0usize;
        let mut depth = 0usize;
        for e in &subs {
            let (Some(b), Some(n)) = (e.wire_bound_bytes().get(), e.depth().get()) else {
                self.note(format!(
                    "subscription `{}` states {}, so the subscriber ring keeps its defaults: \
                     the pool is COUNT x DEPTH x BOUND and half a formula prices no image",
                    e.topic,
                    missing_half(e)
                ));
                return;
            };
            // A depth of 0 is this backend's SYSTEM_DEFAULT sentinel on the
            // wire (`session.c` leaves it unresolved so the Agent's DDS layer
            // supplies its own), not a queue of nothing. It cannot size a ring.
            if n == 0 {
                self.note(format!(
                    "subscription `{}` states `depth = 0`, which is the defer-to-the-Agent \
                     sentinel rather than a queue length, so the subscriber ring keeps its \
                     defaults",
                    e.topic
                ));
                return;
            }
            bound = bound.max(b);
            depth = depth.max(n as usize);
        }
        self.values
            .insert(ENV_SUBSCRIBER_BUFFER, bound + CDR_HEADER_LEN);
        self.values.insert(ENV_SUBSCRIBER_RING_DEPTH, depth);
    }
}

/// Which half of the product this endpoint is missing, in prose.
fn missing_half(e: &Endpoint) -> String {
    let mut missing = Vec::new();
    if !e.wire_bound_bytes().is_stated() {
        missing.push(match e.wire_bound_bytes().refusal() {
            Some(r) => format!("no wire bound ({r})"),
            None => "no wire bound".to_string(),
        });
    }
    if !e.depth().is_stated() {
        missing.push(match e.depth().refusal() {
            Some(r) => format!("no depth ({r})"),
            None => "no depth".to_string(),
        });
    }
    missing.join(" and ")
}

/// The descriptor this build was pointed at, or `None`.
///
/// Same three outcomes as `nros-node/build.rs` (phase-454 W4) and for the same
/// reasons: no descriptor keeps every literal, a refused field keeps its
/// literal loudly, and a descriptor that does not parse — or names a schema
/// this reader does not know — is a hard build error. A descriptor EXISTS in
/// that last case, so defaulting would size from numbers a user believes they
/// supplied.
fn sizing_descriptor() -> Option<SizingDescriptor> {
    match nros_sizing_descriptor::from_build_env() {
        Ok(d) => d,
        Err(e) => panic!("nros-rmw-xrce-cffi: {e}"),
    }
}

// --- negative controls, run on the normal path -----------------------------

fn xrce_demand_selftest() {
    use nros_sizing_descriptor::{History, RegistrationPath, Status};

    // A descriptor shaped like a real one: contract basis, nothing undeclared.
    let image = |eps: Vec<Endpoint>| -> SizingDescriptor {
        let mut d = SizingDescriptor::new("selftest", Status::Derived, Basis::Contract);
        d.meta.set_undeclared_endpoints(Some(0));
        d.endpoints = eps;
        d
    };
    let endpoint = |kind: EndpointKind,
                    topic: &str,
                    rel: Option<Reliability>,
                    depth: Option<u32>,
                    bound: Option<usize>| {
        let mut e = Endpoint::new(kind, "std_msgs/msg/String", topic);
        e.set_history(Some(History::KeepLast))
            .set_reliability(rel)
            .set_depth(depth)
            .set_wire_bound_bytes(bound)
            .set_registration_path(Some(RegistrationPath::RustTypedSchemaless));
        e
    };
    let be = Some(Reliability::BestEffort);

    // 1. No descriptor derives nothing. Every existing image builds unchanged.
    let none = XrceDemand::derive(None);
    assert!(
        none.values.is_empty(),
        "a build with no descriptor must move no knob"
    );
    assert!(none.notes.is_empty(), "and must say nothing about it");

    // 2. Best-effort-only pub/sub: the reliable streams drop to the protocol
    //    floor. The wave's headline number.
    let d = XrceDemand::derive(Some(&image(vec![
        endpoint(EndpointKind::Publisher, "/chatter", be, Some(1), Some(1170)),
        endpoint(EndpointKind::Subscription, "/echo", be, Some(1), Some(1170)),
    ])));
    assert_eq!(
        d.for_knob(ENV_STREAM_HISTORY),
        Some(RELIABLE_CONTROL_HISTORY),
        "every endpoint declared best_effort, so the reliable streams carry control only"
    );

    // 3. ONE reliable endpoint and the whole image pays, because one stream
    //    serves them all.
    let d = XrceDemand::derive(Some(&image(vec![
        endpoint(EndpointKind::Publisher, "/chatter", be, Some(1), Some(1170)),
        endpoint(
            EndpointKind::Subscription,
            "/echo",
            Some(Reliability::Reliable),
            Some(1),
            Some(1170),
        ),
    ])));
    assert_eq!(d.for_knob(ENV_STREAM_HISTORY), None);

    // 4. Undeclared reliability is ASSUMED RELIABLE — the safe direction. A
    //    mutation to "assume best_effort" is a silent under-size on the one
    //    path that retransmits, and it would pass tests 2 and 3.
    let d = XrceDemand::derive(Some(&image(vec![
        endpoint(EndpointKind::Publisher, "/chatter", be, Some(1), Some(1170)),
        endpoint(
            EndpointKind::Subscription,
            "/echo",
            None,
            Some(1),
            Some(1170),
        ),
    ])));
    assert_eq!(d.for_knob(ENV_STREAM_HISTORY), None);
    assert!(
        d.notes.iter().any(|n| n.contains("assumed reliable")),
        "and it says so: {:?}",
        d.notes
    );

    // 5. A service is reliable by construction, so a service image keeps the
    //    full history even with best_effort on everything it declares.
    let d = XrceDemand::derive(Some(&image(vec![endpoint(
        EndpointKind::ServiceServer,
        "/add",
        be,
        Some(10),
        Some(64),
    )])));
    assert_eq!(d.for_knob(ENV_STREAM_HISTORY), None);

    // 6. The subscriber family: both factors, from the declaration.
    let d = XrceDemand::derive(Some(&image(vec![
        endpoint(EndpointKind::Subscription, "/a", be, Some(10), Some(1170)),
        endpoint(EndpointKind::Subscription, "/b", be, Some(4), Some(96)),
    ])));
    assert_eq!(
        d.for_knob(ENV_SUBSCRIBER_BUFFER),
        Some(1170 + CDR_HEADER_LEN),
        "the entry must hold the largest declared payload PLUS its CDR header"
    );
    assert_eq!(
        d.for_knob(ENV_SUBSCRIBER_RING_DEPTH),
        Some(10),
        "the ring must hold the deepest declared queue"
    );

    // 7. THE COUPLING. Bounds without depths derives NEITHER. Mutating this to
    //    emit the buffer alone makes the pool 15% LARGER than the defaults it
    //    replaced (1,174 x 32 against 1,024 x 32) — a regression that builds,
    //    boots and passes every knob gate.
    let d = XrceDemand::derive(Some(&image(vec![endpoint(
        EndpointKind::Subscription,
        "/a",
        be,
        None,
        Some(1170),
    )])));
    assert_eq!(d.for_knob(ENV_SUBSCRIBER_BUFFER), None);
    assert_eq!(d.for_knob(ENV_SUBSCRIBER_RING_DEPTH), None);
    // The reliability half is a different fact and must NOT be degraded by it
    // — D6: a refusal "never degrades another consumer's facts".
    assert_eq!(
        d.for_knob(ENV_STREAM_HISTORY),
        Some(RELIABLE_CONTROL_HISTORY)
    );

    // 8. An action opens subscriptions this table does not itemise.
    let d = XrceDemand::derive(Some(&image(vec![
        endpoint(EndpointKind::Subscription, "/a", be, Some(10), Some(1170)),
        endpoint(EndpointKind::ActionClient, "/fib", be, Some(10), Some(96)),
    ])));
    assert_eq!(d.for_knob(ENV_SUBSCRIBER_BUFFER), None);
    assert_eq!(d.for_knob(ENV_SUBSCRIBER_RING_DEPTH), None);

    // 9. "Absence is not zero": one silent endpoint refuses every derivation,
    //    including the reliability one the others would have satisfied.
    let mut d9 = image(vec![endpoint(
        EndpointKind::Subscription,
        "/a",
        be,
        Some(10),
        Some(1170),
    )]);
    d9.meta.set_undeclared_endpoints(Some(1));
    let d = XrceDemand::derive(Some(&d9));
    assert!(d.values.is_empty(), "{:?}", d.values);
    assert!(!d.notes.is_empty(), "and it is loud about it");

    // 10. A closure basis sizes nothing.
    let mut d10 = image(vec![endpoint(
        EndpointKind::Subscription,
        "/a",
        be,
        Some(10),
        Some(1170),
    )]);
    d10.meta.basis = Basis::Closure;
    assert!(XrceDemand::derive(Some(&d10)).values.is_empty());

    // 11. Zero survives unfloored (D7, issues 1015/1033). An empty message is
    //     the CDR header and nothing else, and the floor that keeps that legal
    //     lives at the array rather than here.
    let d = XrceDemand::derive(Some(&image(vec![endpoint(
        EndpointKind::Subscription,
        "/tick",
        be,
        Some(1),
        Some(0),
    )])));
    assert_eq!(d.for_knob(ENV_SUBSCRIBER_BUFFER), Some(CDR_HEADER_LEN));
    assert_eq!(d.for_knob(ENV_SUBSCRIBER_RING_DEPTH), Some(1));
}
