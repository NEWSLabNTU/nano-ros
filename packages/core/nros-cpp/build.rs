fn main() {
    nros_build_helpers::cpp::run();
    generate_cpp_surface_anchor();
}

/// Phase 241 W11 (Option D) — emit a `#[used]` anchor that force-references every
/// `#[unsafe(no_mangle)] extern "C"` entry point this crate defines.
///
/// Mirrors `nros-c`'s `generate_c_surface_anchor`, one level up. Today `nros-cpp` is the
/// C++ umbrella's staticlib root, so its own `nros_cpp_*` no_mangle symbols survive DCE
/// for free. Under W11 a per-entry `<entry>_runtime` crate bundles `nros-cpp` as an
/// **rlib** dependency and emits the final staticlib; rustc then dead-code-eliminates
/// any `nros_cpp_*` symbol the runtime root doesn't reference — yet the C++ entry / node
/// TUs (linked separately into the exe) call them, so they must be in the archive. The
/// anchor (re-pulled by the runtime root via `FORCE_LINK_ANCHOR`) keeps the whole C++
/// FFI surface. (We never call through it; the extern decls only name the symbols.)
/// Is cargo feature `feat` enabled for this build? cargo exports `CARGO_FEATURE_<NAME>`
/// (uppercased, `-` → `_`) for every enabled feature.
fn cargo_feature_enabled(feat: &str) -> bool {
    let env = format!("CARGO_FEATURE_{}", feat.to_uppercase().replace('-', "_"));
    std::env::var_os(env).is_some()
}

/// True when `path` belongs to a feature-gated module that is INACTIVE in this build —
/// either gated by a non-feature cfg (e.g. `#[cfg(cbindgen)]`, empty feature list) or by
/// features none of which is enabled. Matches the module name against the file stem and
/// every parent directory component (a `mod action;` may be a directory of submodules).
fn path_in_inactive_module(
    path: &std::path::Path,
    gated: &std::collections::BTreeMap<String, Vec<String>>,
) -> bool {
    for comp in path.components() {
        let std::path::Component::Normal(os) = comp else {
            continue;
        };
        let Some(s) = os.to_str() else { continue };
        let stem = s.strip_suffix(".rs").unwrap_or(s);
        if let Some(feats) = gated.get(stem)
            && !feats.iter().any(|f| cargo_feature_enabled(f))
        {
            return true;
        }
    }
    false
}

/// Parse `<lib_rs>` for `[#[cfg(...)]] mod <name>;` declarations, returning a map of
/// module name → the `feature = "X"` names appearing in its `#[cfg(...)]` (empty/absent
/// when ungated). Only `mod <name>;` (separate-file) declarations matter; inline
/// `mod <name> { … }` bodies have their symbols indented and are skipped by the column-0
/// heuristic. Handles `feature = "X"`, `any(...)`, `all(...)` uniformly by collecting
/// every `feature = "X"` token (a file is kept when ANY listed feature is on — loose for
/// `all`, but nros-cpp gates each module on a single feature).
fn parse_gated_modules(lib_rs: &str) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut map = std::collections::BTreeMap::new();
    let Ok(content) = std::fs::read_to_string(lib_rs) else {
        return map;
    };
    let lines: Vec<&str> = content.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        // `mod <name>;` (no body) at column 0 — a separate-file module.
        let Some(rest) = t.strip_prefix("mod ") else {
            continue;
        };
        if line.starts_with(' ') || line.starts_with('\t') {
            continue; // indented => inline/nested, not a top-level file module
        }
        let Some(name) = rest.strip_suffix(';') else {
            continue; // `mod x { … }` body, not `mod x;`
        };
        let name = name.trim().to_string();
        // Scan the contiguous attribute block directly above for a `#[cfg(...)]` and
        // collect any `feature = "X"` tokens it names.
        let mut saw_cfg = false;
        let mut feats = Vec::new();
        let mut j = i;
        while j > 0 {
            j -= 1;
            let a = lines[j].trim_start();
            if a.is_empty() || a.starts_with("//") {
                continue;
            }
            if !a.starts_with("#[") {
                break; // left the attribute block
            }
            if a.contains("cfg(") {
                saw_cfg = true;
                let mut s = a;
                while let Some(p) = s.find("feature = \"") {
                    let after = &s[p + "feature = \"".len()..];
                    if let Some(end) = after.find('"') {
                        feats.push(after[..end].to_string());
                        s = &after[end + 1..];
                    } else {
                        break;
                    }
                }
            }
        }
        // Record the module whenever it carries ANY `#[cfg(...)]`. The skip rule is "active
        // iff some named feature is enabled" — so a module gated only by a non-feature cfg
        // (e.g. `#[cfg(cbindgen)]`, header-only) records an EMPTY feature list and is always
        // skipped for the link-time anchor. Feature-gated modules are kept iff their feature
        // is on.
        if saw_cfg {
            map.insert(name, feats);
        }
    }
    map
}

fn generate_cpp_surface_anchor() {
    use std::fmt::Write as _;

    // Map each separate-file module to the cargo features that gate it, parsed from
    // lib.rs's `#[cfg(...)] mod <name>;` declarations. nros-cpp's C++ FFI surface lives in
    // per-file modules gated by `#[cfg(feature = "rmw-cffi")]` etc. — their `#[no_mangle]`
    // fns sit at column 0 of THEIR file with no fn-local `#[cfg]`, so the column-0 /
    // has_cfg heuristic alone can't tell they're gated. Anchoring them when the gating
    // feature is OFF produces `undefined symbol` at link (the module wasn't compiled). So
    // skip a file whose module is gated and NONE of its features is enabled in this build.
    let gated_modules = parse_gated_modules("src/lib.rs");

    let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut stack = vec![std::path::PathBuf::from("src")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // Skip a feature-gated module when none of its gating features is on. Match the
            // module name against the file stem AND every parent dir component (a gated
            // `mod action;` may be a directory `action/…` with submodule files).
            if path_in_inactive_module(&path, &gated_modules) {
                continue;
            }
            println!("cargo:rerun-if-changed={}", path.display());
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            // Walk the contiguous attribute / doc block above each `extern "C" fn` and
            // anchor it only when that block carries `#[unsafe(no_mangle)]` AND no
            // `#[cfg(...)]`, AND the fn is declared at column 0 (an indented fn may be
            // gated by an enclosing `#[cfg] mod { … }` even when its own block isn't).
            // Same heuristic as nros-c — see that crate's build.rs for the rationale.
            let mut has_no_mangle = false;
            let mut has_cfg = false;
            for line in content.lines() {
                let t = line.trim_start();
                let is_attr_or_doc =
                    t.starts_with("#[") || t.starts_with("#!") || t.starts_with("//");
                if t.contains("no_mangle") {
                    has_no_mangle = true;
                }
                if t.starts_with("#[cfg(") || t.starts_with("#[cfg_attr(") {
                    has_cfg = true;
                }
                if let Some(idx) = t.find("extern \"C\" fn ") {
                    let indented = line.starts_with(' ') || line.starts_with('\t');
                    if has_no_mangle && !has_cfg && !indented {
                        let rest = &t[idx + "extern \"C\" fn ".len()..];
                        let name: String = rest
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        if name.starts_with("nros_") {
                            names.insert(name);
                        }
                    }
                    has_no_mangle = false;
                    has_cfg = false;
                } else if !is_attr_or_doc && !t.is_empty() {
                    has_no_mangle = false;
                    has_cfg = false;
                }
            }
        }
    }

    let mut out = String::new();
    out.push_str("// Generated by build.rs — do not edit. See generate_cpp_surface_anchor().\n");
    out.push_str("#[allow(unused, improper_ctypes)]\n");
    // `pub` so a downstream staticlib root (the per-entry `<entry>_runtime` crate) can
    // REFERENCE the anchor — a bare `#[used]` here is DCE'd when nros-cpp is a dep rlib.
    out.push_str("pub mod cpp_surface_anchor {\n    unsafe extern \"C\" {\n");
    for n in &names {
        let _ = writeln!(out, "        pub fn {n}();");
    }
    out.push_str("    }\n");
    let _ = writeln!(
        out,
        "    #[used]\n    pub static CPP_SURFACE_ANCHOR: [unsafe extern \"C\" fn(); {}] = [",
        names.len()
    );
    for n in &names {
        let _ = writeln!(out, "        {n},");
    }
    out.push_str("    ];\n}\n");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest = std::path::Path::new(&out_dir).join("cpp_surface_anchor.rs");
    std::fs::write(&dest, out).expect("write cpp_surface_anchor.rs");
}
