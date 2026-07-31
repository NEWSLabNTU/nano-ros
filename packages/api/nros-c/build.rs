fn main() {
    nros_build_helpers::c::run();
    generate_c_surface_anchor();
}

/// Is cargo feature `feat` enabled for this build? cargo exports `CARGO_FEATURE_<NAME>`
/// (uppercased, `-` → `_`) for every enabled feature.
fn cargo_feature_enabled(feat: &str) -> bool {
    let env = format!("CARGO_FEATURE_{}", feat.to_uppercase().replace('-', "_"));
    std::env::var_os(env).is_some()
}

/// True when `path` belongs to a feature-gated module INACTIVE in this build — gated by a
/// non-feature cfg (e.g. `#[cfg(cbindgen)]`, header-only) or by features none of which is
/// enabled. Matches the module name against the file stem and every parent dir component
/// (a `mod action;` may be a directory `action/…` of submodule files).
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

/// Parse `<lib_rs>` for `[#[cfg(...)]] mod <name>;` declarations → map of module name → the
/// `feature = "X"` names in its `#[cfg(...)]` (empty when gated only by a non-feature cfg
/// like `cbindgen`). A module with ANY cfg is recorded; the link-time anchor skips it
/// unless one of its named features is enabled (empty list ⇒ always skipped).
fn parse_gated_modules(lib_rs: &str) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut map = std::collections::BTreeMap::new();
    let Ok(content) = std::fs::read_to_string(lib_rs) else {
        return map;
    };
    let lines: Vec<&str> = content.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
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
                break;
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
        if saw_cfg {
            map.insert(name, feats);
        }
    }
    map
}

/// Phase 241.D3-rev — emit a `#[used]` anchor that force-references every
/// `#[unsafe(no_mangle)] extern "C"` entry point in this crate.
///
/// `nros-c` is bundled as an **rlib** dependency by the C++ umbrella (`nros-cpp`,
/// which links only `libnros_cpp.a`). rustc's staticlib emission dead-code-eliminates
/// no_mangle symbols from a dependency rlib that the root crate does not reference, so
/// a C++ binary calling a C-API function the C++ FFI itself never touches (e.g.
/// `nros_param_server_fini`) fails at link with `undefined reference`. The anchor —
/// `#[used]`, so it survives DCE in every consumer — takes the address of each entry
/// point, keeping the whole C surface in the archive. (We never call through it; the
/// extern declarations only name the symbols, so their argument types are irrelevant.)
fn generate_c_surface_anchor() {
    use std::fmt::Write as _;

    // Modules gated by `#[cfg(...)] mod <name>;` in lib.rs (e.g. the `#[cfg(cbindgen)]`
    // header-only action/config/event/… modules, or feature-gated ones). Their column-0
    // `#[no_mangle]` fns must NOT be anchored when the gate is inactive — they aren't
    // compiled, so referencing them is an `undefined symbol` at link.
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
            if path_in_inactive_module(&path, &gated_modules) {
                continue;
            }
            println!("cargo:rerun-if-changed={}", path.display());
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            // Walk the contiguous attribute / doc block above each `extern "C" fn`.
            // Anchor it only if that block has `#[unsafe(no_mangle)]` AND no
            // `#[cfg(...)]` — a feature-gated fn may not be compiled in the umbrella's
            // feature set, so referencing it would create an undefined symbol. Ungated
            // entry points (the always-present C surface, e.g. nros_param_server_fini)
            // are exactly what a C++ binary may call but the C++ FFI never references.
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
                    // Only anchor TOP-LEVEL (column-0) entry points. A `#[cfg(...)]`
                    // gate often sits on the ENCLOSING module/block (e.g.
                    // `#[cfg(feature = "param-services")] mod … { … pub extern "C" fn
                    // … }`), not the fn's own attribute block — so an indented fn may
                    // be feature-gated even when `has_cfg` is false. Anchoring such a
                    // symbol when its feature is off creates an undefined reference.
                    // The ungated C surface is declared at column 0; require that.
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
                    // A non-attribute, non-fn line (a static, a struct, code) ends the
                    // current attribute block — reset the pending flags.
                    has_no_mangle = false;
                    has_cfg = false;
                }
            }
        }
    }

    let mut out = String::new();
    out.push_str("// Generated by build.rs — do not edit. See generate_c_surface_anchor().\n");
    out.push_str("#[allow(unused, improper_ctypes)]\n");
    // `pub` so the C++ umbrella (`nros-cpp`) can REFERENCE the anchor from its own
    // staticlib root — a bare `#[used]` here is DCE'd when nros-c is a dependency rlib.
    out.push_str("pub mod c_surface_anchor {\n    unsafe extern \"C\" {\n");
    for n in &names {
        let _ = writeln!(out, "        pub fn {n}();");
    }
    out.push_str("    }\n");
    let _ = writeln!(
        out,
        "    #[used]\n    pub static C_SURFACE_ANCHOR: [unsafe extern \"C\" fn(); {}] = [",
        names.len()
    );
    for n in &names {
        let _ = writeln!(out, "        {n},");
    }
    out.push_str("    ];\n}\n");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest = std::path::Path::new(&out_dir).join("c_surface_anchor.rs");
    std::fs::write(&dest, out).expect("write c_surface_anchor.rs");
}
