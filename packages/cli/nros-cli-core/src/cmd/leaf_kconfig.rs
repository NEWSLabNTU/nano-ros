//! phase-481 W1 (RFC-0098 D11) — an image's configuration rendered as a Zephyr
//! Kconfig fragment.
//!
//! On Zephyr the image's `system.toml` is the ONE source of its nano-ros
//! configuration (D10), and Kconfig is only the MECHANISM that carries it into
//! the build: the nano-ros module's `module_ext_root` hook
//! (`zephyr/modules/modules.cmake`) asks `nros ws leaf-system --kconfig-out`
//! for this fragment and places it last in `EXTRA_CONF_FILE`.
//!
//! What the fragment states, and from where:
//!
//! | rows | source |
//! |---|---|
//! | `CONFIG_NROS_RMW_<X>=y` | the image's `rmw` |
//! | `CONFIG_NROS_{C,CPP,RUST}_API=y` | the package's language |
//! | `CONFIG_NROS_ZENOH_LOCATOR` / `CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}` | the image's `locator` |
//! | one row per `[image.<id>] env` row whose Kconfig symbol EXISTS | the image's `env` (and what its `transport` implies) |
//!
//! The env↔Kconfig pairing is `nros_zephyr_build::kconfig_key_for` — the
//! `KCONFIG_PAIRS` table the build scripts' own ladder reads — never a second
//! table here. Which symbols exist, and their types, is read from the module's
//! own `zephyr/Kconfig`, because Zephyr REFUSES a fragment that assigns an
//! undefined symbol (`warn_assign_undef`, a warning turned error) and a value
//! that does not fit the symbol's type. Both refusals happen here instead, at
//! render time, naming the `system.toml` row rather than a generated file.
//!
//! An env row with NO Kconfig symbol is not lost: it is returned in
//! [`Rendered::cargo_rows`], which the caller writes as issue 1712's unforced
//! `--config` `[env]` file for the Zephyr cargo commands.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use eyre::{Result, eyre};
use nros_lang::Language;
use nros_orchestration_ir::leaf_system::LeafSystem;

/// A Kconfig symbol's value type, as `zephyr/Kconfig` declares it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymType {
    Bool,
    Int,
    Hex,
    String,
}

/// One symbol: its type, and the `if NROS_RMW_<X>` block it sits in, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub ty: SymType,
    /// The RMW choice symbol (`NROS_RMW_ZENOH`, …) the definition is guarded
    /// by. Assigning a symbol whose guard is off is a Zephyr refusal ("was
    /// assigned the value … but got the value ''"), so it is refused here.
    pub rmw_guard: Option<String>,
}

/// The symbols `zephyr/Kconfig` defines, keyed WITHOUT the `CONFIG_` prefix.
#[derive(Clone, Debug, Default)]
pub struct KconfigSymbols {
    syms: BTreeMap<String, Symbol>,
}

/// The module's Kconfig file, relative to the nano-ros root.
pub const KCONFIG_FILE: &str = "zephyr/Kconfig";

impl KconfigSymbols {
    /// Read `<nano_ros_root>/zephyr/Kconfig`.
    pub fn load(nano_ros_root: &Path) -> Result<Self> {
        let path = nano_ros_root.join(KCONFIG_FILE);
        let text = std::fs::read_to_string(&path).map_err(|e| eyre!("{}: {e}", path.display()))?;
        Ok(Self::parse(&text))
    }

    /// The subset of Kconfig this file uses: `config`/`menuconfig` blocks, a
    /// type line, and `if`/`endif` nesting. A choice member is a `bool`.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut syms = BTreeMap::new();
        let mut ifs: Vec<String> = Vec::new();
        let mut pending: Option<String> = None;
        for raw in text.lines() {
            let line = raw.trim();
            let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let rest = rest.trim();
            match word {
                "config" | "menuconfig" => {
                    pending = Some(rest.to_string());
                    continue;
                }
                "if" => ifs.push(rest.split_whitespace().next().unwrap_or("").to_string()),
                "endif" => {
                    ifs.pop();
                }
                _ => {}
            }
            let Some(name) = pending.as_ref() else {
                continue;
            };
            let ty = match word {
                "bool" | "tristate" | "def_bool" | "def_tristate" => Some(SymType::Bool),
                "int" | "def_int" => Some(SymType::Int),
                "hex" | "def_hex" => Some(SymType::Hex),
                "string" | "def_string" => Some(SymType::String),
                _ => None,
            };
            if let Some(ty) = ty {
                let rmw_guard = ifs
                    .iter()
                    .rev()
                    .find(|c| c.starts_with("NROS_RMW_"))
                    .cloned();
                syms.insert(name.clone(), Symbol { ty, rmw_guard });
                pending = None;
            }
        }
        Self { syms }
    }

    /// The symbol `CONFIG_<name>` names, if Kconfig defines it.
    #[must_use]
    pub fn get(&self, config_symbol: &str) -> Option<&Symbol> {
        self.syms.get(
            config_symbol
                .strip_prefix("CONFIG_")
                .unwrap_or(config_symbol),
        )
    }
}

/// One rendered `CONFIG_<SYM>=<value>` line and the `system.toml` statement it
/// came from (written beside it as a comment).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub symbol: String,
    pub value: String,
    pub from: String,
}

/// The rendering: the fragment's lines, plus the env rows no Kconfig symbol
/// carries (issue 1712's `--config` file).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rendered {
    pub lines: Vec<Line>,
    pub cargo_rows: BTreeMap<String, String>,
}

/// The RMW choice symbol an image's `rmw` selects.
pub fn rmw_symbol(rmw: &str) -> Result<&'static str, String> {
    RMW_CHOICE
        .iter()
        .find(|(word, _)| *word == rmw)
        .map(|(_, sym)| *sym)
        .ok_or_else(|| {
            format!(
                "`rmw = \"{rmw}\"` has no Zephyr backend — the Zephyr module offers \
                 zenoh, xrce and cyclonedds"
            )
        })
}

/// The image's `rmw` word and the `choice NROS_RMW_BACKEND` member it selects.
const RMW_CHOICE: &[(&str, &str)] = &[
    ("zenoh", "NROS_RMW_ZENOH"),
    ("xrce", "NROS_RMW_XRCE"),
    ("cyclonedds", "NROS_RMW_CYCLONEDDS"),
];

/// The API choice symbol a package language selects.
#[must_use]
pub fn api_symbol(language: Language) -> &'static str {
    match language {
        Language::Rust => "NROS_RUST_API",
        Language::C => "NROS_C_API",
        Language::Cpp => "NROS_CPP_API",
    }
}

/// The language of the ENTRY a Zephyr package builds — which is what the
/// `CONFIG_NROS_*_API` choice selects (the runtime library the entry links):
/// `Cargo.toml` is a `rust_cargo_application()` entry, Rust; a `CMakeLists.txt`
/// package's entry is the TYPED Zephyr carrier, which is C++ whatever its
/// components are written in (`nano_ros_add_node(... TYPED)` drives them through
/// `ZephyrBoard::run_components`). Measured when this was written: all twelve
/// `examples/zephyr/{c,cpp}/*` leaves, `templates/zephyr-byo` and every C/C++
/// workspace image resolve to `CONFIG_NROS_CPP_API` today — the C leaves state
/// `C_API` and then `CPP_API`, and the choice keeps the last. `None` for a
/// directory with neither file; the caller then states no API row.
#[must_use]
pub fn package_language(dir: &Path) -> Option<Language> {
    if dir.join("Cargo.toml").is_file() {
        Some(Language::Rust)
    } else if dir.join("CMakeLists.txt").is_file() {
        Some(Language::Cpp)
    } else {
        None
    }
}

/// `udp/<host>:<port>` or `<host>:<port>` — the XRCE agent endpoint.
fn xrce_endpoint(locator: &str) -> Result<(String, u16), String> {
    let rest = match locator.split_once('/') {
        Some(("udp" | "udp4", rest)) => rest,
        Some((scheme, _)) => {
            return Err(format!(
                "`locator = \"{locator}\"`: the Zephyr XRCE backend dials a UDP agent, \
                 so the locator is `udp/<host>:<port>` (not `{scheme}/…`)"
            ));
        }
        None => locator,
    };
    let (host, port) = rest
        .rsplit_once(':')
        .ok_or_else(|| format!("`locator = \"{locator}\"`: expected `udp/<host>:<port>`"))?;
    let port = port
        .parse::<u16>()
        .map_err(|_| format!("`locator = \"{locator}\"`: `{port}` is not a port"))?;
    if host.is_empty() {
        return Err(format!("`locator = \"{locator}\"`: no host"));
    }
    Ok((host.to_string(), port))
}

/// `value`, written as a value of type `ty`, or why it cannot be.
fn kconfig_value(ty: SymType, value: &str) -> Result<String, String> {
    let v = value.trim();
    match ty {
        SymType::Bool => match v.to_ascii_lowercase().as_str() {
            "1" | "y" | "yes" | "true" | "on" => Ok("y".to_string()),
            "0" | "n" | "no" | "false" | "off" => Ok("n".to_string()),
            _ => Err("a bool — write \"1\" or \"0\"".to_string()),
        },
        SymType::Int => v
            .parse::<i64>()
            .map(|n| n.to_string())
            .map_err(|_| "an int — write a decimal number".to_string()),
        SymType::Hex => {
            let n = match v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
                Some(h) => u64::from_str_radix(h, 16),
                None => v.parse::<u64>(),
            };
            n.map(|n| format!("0x{n:x}"))
                .map_err(|_| "a hex value — write 0x… or a decimal number".to_string())
        }
        SymType::String => Ok(format!(
            "\"{}\"",
            value.replace('\\', "\\\\").replace('"', "\\\"")
        )),
    }
}

/// Render `leaf`'s image. `language` is the package's (see
/// [`package_language`]); `None` states no API row.
pub fn render(
    leaf: &LeafSystem,
    language: Option<Language>,
    syms: &KconfigSymbols,
) -> Result<Rendered, String> {
    let origin = leaf.origin_path().display().to_string();
    let image = leaf.image.clone().unwrap_or_default();
    let mut out = Rendered::default();
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    let mut push = |out: &mut Rendered, sym: &str, value: String, from: String| {
        let symbol = format!("CONFIG_{sym}");
        if let Some(prev) = seen.get(&symbol) {
            return Err(format!(
                "{origin}: `{from}` and `{prev}` both state {symbol} — say it once"
            ));
        }
        seen.insert(symbol.clone(), from.clone());
        out.lines.push(Line {
            symbol,
            value,
            from,
        });
        Ok(())
    };

    // The RMW choice.
    let rmw_sym = match leaf.rmw.as_deref() {
        Some(rmw) => {
            let sym = rmw_symbol(rmw).map_err(|e| format!("{origin}: [image.{image}] {e}"))?;
            push(&mut out, sym, "y".to_string(), format!("rmw = \"{rmw}\""))?;
            Some(sym)
        }
        None => None,
    };

    // The language API.
    if let Some(lang) = language {
        push(
            &mut out,
            api_symbol(lang),
            "y".to_string(),
            format!("the package's entry is {}", lang.as_str()),
        )?;
    }

    // The deploy endpoint.
    if let Some(locator) = leaf.network.locator.as_deref() {
        let from = format!("locator = \"{locator}\"");
        match rmw_sym {
            Some("NROS_RMW_XRCE") => {
                let (host, port) =
                    xrce_endpoint(locator).map_err(|e| format!("{origin}: [image.{image}] {e}"))?;
                push(
                    &mut out,
                    "NROS_XRCE_AGENT_ADDR",
                    kconfig_value(SymType::String, &host)?,
                    from.clone(),
                )?;
                push(&mut out, "NROS_XRCE_AGENT_PORT", port.to_string(), from)?;
            }
            Some("NROS_RMW_ZENOH") => {
                push(
                    &mut out,
                    "NROS_ZENOH_LOCATOR",
                    kconfig_value(SymType::String, locator)?,
                    from,
                )?;
            }
            // Cyclone discovers; an image with no RMW names no backend whose
            // endpoint this could be.
            _ => {}
        }
    }

    // `[image.<id>] env` (after what the image's `transport` implies).
    for (key, value) in crate::cmd::leaf_settings::image_layers(leaf) {
        let symbol = nros_zephyr_build::kconfig_key_for(&key);
        let Some(sym) = syms.get(&symbol) else {
            out.cargo_rows.insert(key, value);
            continue;
        };
        let from = format!("env {key} = \"{value}\"");
        if let (Some(guard), Some(chosen)) = (&sym.rmw_guard, rmw_sym)
            && guard.as_str() != chosen
        {
            return Err(format!(
                "{origin}: [image.{image}] `{from}` sets {symbol}, which Kconfig defines \
                 only under `if {guard}`, and this image's rmw selects {chosen} — Zephyr \
                 would refuse the assignment"
            ));
        }
        let v = kconfig_value(sym.ty, &value).map_err(|why| {
            format!(
                "{origin}: [image.{image}] `{from}`: on Zephyr this knob is {symbol}, \
                 {why}"
            )
        })?;
        let bare = symbol
            .strip_prefix("CONFIG_")
            .unwrap_or(&symbol)
            .to_string();
        push(&mut out, &bare, v, from)?;
    }
    Ok(out)
}

/// The fragment's text, or `None` when there is nothing to state.
#[must_use]
pub fn fragment_text(leaf: &LeafSystem, r: &Rendered) -> Option<String> {
    if r.lines.is_empty() {
        return None;
    }
    let mut s = format!(
        "# GENERATED by nros (`ws leaf-system --kconfig-out`, phase-481 W1 / RFC-0098 D11)\n\
         # -- do not edit. Edit the image in {}\n\
         # ([image.{}]); this file is re-rendered on the next configure.\n",
        leaf.origin_path().display(),
        leaf.image.as_deref().unwrap_or("")
    );
    for l in &r.lines {
        s.push_str(&format!("\n# {}\n{}={}\n", l.from, l.symbol, l.value));
    }
    Some(s)
}

/// Write `text` to `out` only when the bytes differ (so an unchanged image
/// leaves no new mtime for Kconfig's dependency edge), or remove a stale file
/// when there is nothing to state. Returns the path when written.
pub fn write_fragment(out: &Path, text: Option<&str>) -> Result<Option<PathBuf>> {
    let Some(body) = text else {
        match std::fs::remove_file(out) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(eyre!("removing stale {}: {e}", out.display())),
        }
        return Ok(None);
    };
    if std::fs::read_to_string(out).ok().as_deref() != Some(body) {
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(|e| eyre!("{}: {e}", dir.display()))?;
        }
        std::fs::write(out, body).map_err(|e| eyre!("{}: {e}", out.display()))?;
    }
    Ok(Some(out.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_orchestration_ir::leaf_system;

    const KCONFIG: &str = "\
menuconfig NROS
    bool \"nros\"
if NROS
choice NROS_RMW_BACKEND
    prompt \"RMW\"
config NROS_RMW_ZENOH
    bool \"zenoh\"
config NROS_RMW_XRCE
    bool \"xrce\"
endchoice
config NROS_EXECUTOR_MAX_CBS
    int \"cbs\"
    default -1
config NROS_LOG_MAX_LEVEL
    int \"ceiling\"
if NROS_RMW_ZENOH
config NROS_MAX_QUERYABLES
    int \"q\"
config NROS_ZENOH_TX_BATCH
    bool \"batch\"
config NROS_ZENOH_LOCATOR
    string \"loc\"
endif # NROS_RMW_ZENOH
if NROS_RMW_XRCE
config NROS_XRCE_AGENT_ADDR
    string \"addr\"
config NROS_XRCE_AGENT_PORT
    int \"port\"
endif
config NROS_SOME_HEX
    hex \"h\"
endif # NROS
";

    fn leaf(system: &str, extra: &[&str]) -> (tempfile::TempDir, LeafSystem) {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("CMakeLists.txt"), "project(x C)\n").unwrap();
        std::fs::write(td.path().join("system.toml"), system).unwrap();
        for f in extra {
            let p = td.path().join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "").unwrap();
        }
        let l = leaf_system::read(td.path()).unwrap().unwrap();
        (td, l)
    }

    fn rows(r: &Rendered) -> BTreeMap<String, String> {
        r.lines
            .iter()
            .map(|l| (l.symbol.clone(), l.value.clone()))
            .collect()
    }

    #[test]
    fn the_parser_reads_types_and_rmw_guards() {
        let s = KconfigSymbols::parse(KCONFIG);
        assert_eq!(
            s.get("CONFIG_NROS_EXECUTOR_MAX_CBS").unwrap().ty,
            SymType::Int
        );
        assert_eq!(s.get("NROS_RMW_XRCE").unwrap().ty, SymType::Bool);
        assert_eq!(s.get("NROS_SOME_HEX").unwrap().ty, SymType::Hex);
        let q = s.get("NROS_MAX_QUERYABLES").unwrap();
        assert_eq!(q.rmw_guard.as_deref(), Some("NROS_RMW_ZENOH"));
        assert_eq!(
            s.get("NROS_XRCE_AGENT_PORT").unwrap().rmw_guard.as_deref(),
            Some("NROS_RMW_XRCE"),
            "an `endif` with no comment closes the block too"
        );
        assert_eq!(s.get("NROS_SOME_HEX").unwrap().rmw_guard, None);
        assert!(s.get("NROS_NOPE").is_none());
    }

    /// The real module Kconfig parses, and every `KCONFIG_PAIRS` row whose
    /// symbol it defines is found — the renderer reads the same file Zephyr
    /// does.
    #[test]
    fn the_module_kconfig_defines_the_symbols_the_renderer_names() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let s = KconfigSymbols::load(root).unwrap();
        for sym in [
            "NROS_RMW_ZENOH",
            "NROS_RMW_XRCE",
            "NROS_RMW_CYCLONEDDS",
            "NROS_C_API",
            "NROS_CPP_API",
            "NROS_RUST_API",
            "NROS_ZENOH_LOCATOR",
            "NROS_XRCE_AGENT_ADDR",
            "NROS_XRCE_AGENT_PORT",
        ] {
            assert!(s.get(sym).is_some(), "{sym} missing from zephyr/Kconfig");
        }
        assert_eq!(
            s.get(&nros_zephyr_build::kconfig_key_for("ZPICO_MAX_QUERYABLES"))
                .unwrap()
                .rmw_guard
                .as_deref(),
            Some("NROS_RMW_ZENOH")
        );
        assert_eq!(
            s.get("NROS_EXECUTOR_MAX_CBS").unwrap().ty,
            SymType::Int,
            "a derived-identical knob resolves without a KCONFIG_PAIRS row"
        );
    }

    /// Each row kind: RMW, language API, endpoint, a paired env row (different
    /// words), a derived-identical env row, a bool conversion, and a row with
    /// no Kconfig symbol, which goes to the cargo rows instead.
    #[test]
    fn every_row_kind_renders() {
        let syms = KconfigSymbols::parse(KCONFIG);
        let (_d, l) = leaf(
            "[system]\nname = \"t\"\nrmw = \"zenoh\"\n\n\
             [image.zephyr]\nboard = \"zephyr\"\nlocator = \"tcp/10.0.2.2:7447\"\n\
             env = { NROS_EXECUTOR_MAX_CBS = \"13\", ZPICO_MAX_QUERYABLES = \"9\", \
             ZPICO_TX_BATCH = \"1\", NROS_LINK_IP = \"0\" }\n",
            &[],
        );
        let r = render(&l, Some(Language::C), &syms).unwrap();
        let m = rows(&r);
        assert_eq!(m["CONFIG_NROS_RMW_ZENOH"], "y");
        assert_eq!(m["CONFIG_NROS_C_API"], "y");
        assert_eq!(m["CONFIG_NROS_ZENOH_LOCATOR"], "\"tcp/10.0.2.2:7447\"");
        assert_eq!(m["CONFIG_NROS_EXECUTOR_MAX_CBS"], "13");
        assert_eq!(
            m["CONFIG_NROS_MAX_QUERYABLES"], "9",
            "through KCONFIG_PAIRS"
        );
        assert_eq!(m["CONFIG_NROS_ZENOH_TX_BATCH"], "y", "a bool takes y/n");
        assert!(!m.contains_key("CONFIG_NROS_LINK_IP"));
        assert_eq!(
            r.cargo_rows,
            BTreeMap::from([("NROS_LINK_IP".to_string(), "0".to_string())]),
            "no Kconfig symbol: the --config file carries it"
        );
        let text = fragment_text(&l, &r).unwrap();
        assert!(
            text.contains(
                "# env NROS_EXECUTOR_MAX_CBS = \"13\"\nCONFIG_NROS_EXECUTOR_MAX_CBS=13\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("# rmw = \"zenoh\"\nCONFIG_NROS_RMW_ZENOH=y\n"),
            "{text}"
        );
    }

    #[test]
    fn an_xrce_locator_becomes_the_agent_endpoint() {
        let syms = KconfigSymbols::parse(KCONFIG);
        let (_d, l) = leaf(
            "[system]\nname = \"t\"\n\n[image.zephyr]\nboard = \"zephyr\"\nrmw = \"xrce\"\n\
             locator = \"udp/192.0.2.7:8888\"\n",
            &[],
        );
        let m = rows(&render(&l, None, &syms).unwrap());
        assert_eq!(m["CONFIG_NROS_RMW_XRCE"], "y");
        assert_eq!(m["CONFIG_NROS_XRCE_AGENT_ADDR"], "\"192.0.2.7\"");
        assert_eq!(m["CONFIG_NROS_XRCE_AGENT_PORT"], "8888");
        assert!(!m.contains_key("CONFIG_NROS_ZENOH_LOCATOR"));
        assert!(
            !m.keys().any(|k| k.ends_with("_API")),
            "no language, no row"
        );

        let (_d, bad) = leaf(
            "[system]\nname = \"t\"\n\n[image.zephyr]\nboard = \"zephyr\"\nrmw = \"xrce\"\n\
             locator = \"tcp/1.2.3.4:7447\"\n",
            &[],
        );
        let e = render(&bad, None, &syms).unwrap_err();
        assert!(e.contains("udp/<host>:<port>"), "{e}");
    }

    /// The refusals name the row: a value the symbol's type cannot hold, a
    /// symbol another RMW's `if` guards, and one symbol stated twice.
    #[test]
    fn a_row_zephyr_would_refuse_is_refused_by_name() {
        let syms = KconfigSymbols::parse(KCONFIG);
        let (_d, l) = leaf(
            "[system]\nname = \"t\"\nrmw = \"zenoh\"\n\n[image.zephyr]\nboard = \"zephyr\"\n\
             env = { NROS_LOG_MAX_LEVEL = \"warn\" }\n",
            &[],
        );
        let e = render(&l, None, &syms).unwrap_err();
        assert!(
            e.contains("NROS_LOG_MAX_LEVEL = \"warn\"") && e.contains("an int"),
            "{e}"
        );

        let (_d, l) = leaf(
            "[system]\nname = \"t\"\nrmw = \"xrce\"\n\n[image.zephyr]\nboard = \"zephyr\"\n\
             env = { ZPICO_MAX_QUERYABLES = \"4\" }\n",
            &[],
        );
        let e = render(&l, None, &syms).unwrap_err();
        assert!(
            e.contains("if NROS_RMW_ZENOH") && e.contains("NROS_RMW_XRCE"),
            "{e}"
        );

        let (_d, l) = leaf(
            "[system]\nname = \"t\"\nrmw = \"xrce\"\n\n[image.zephyr]\nboard = \"zephyr\"\n\
             locator = \"udp/1.2.3.4:2018\"\nenv = { NROS_XRCE_AGENT_PORT = \"9\" }\n",
            &[],
        );
        let e = render(&l, None, &syms).unwrap_err();
        assert!(e.contains("both state CONFIG_NROS_XRCE_AGENT_PORT"), "{e}");

        let (_d, l) = leaf(
            "[system]\nname = \"t\"\nrmw = \"fastdds\"\n\n[image.zephyr]\nboard = \"zephyr\"\n",
            &[],
        );
        assert!(
            render(&l, None, &syms)
                .unwrap_err()
                .contains("no Zephyr backend")
        );
    }

    /// The empty case: an image that states nothing renders nothing, and a
    /// stale fragment is removed; an unchanged fragment keeps its mtime.
    #[test]
    fn an_image_stating_nothing_writes_no_fragment() {
        let syms = KconfigSymbols::parse(KCONFIG);
        let (d, l) = leaf(
            "[system]\nname = \"t\"\n\n[image.zephyr]\nboard = \"zephyr\"\n",
            &[],
        );
        let r = render(&l, None, &syms).unwrap();
        assert_eq!(r, Rendered::default());
        assert_eq!(fragment_text(&l, &r), None);
        let out = d.path().join("b/nros/zephyr.conf");
        assert_eq!(
            write_fragment(&out, Some("X=1\n")).unwrap().as_deref(),
            Some(out.as_path())
        );
        let before = std::fs::metadata(&out).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_fragment(&out, Some("X=1\n")).unwrap();
        assert_eq!(std::fs::metadata(&out).unwrap().modified().unwrap(), before);
        assert_eq!(write_fragment(&out, None).unwrap(), None);
        assert!(!out.exists());
    }

    #[test]
    fn the_entry_language_comes_from_the_package_shape() {
        let (d, _) = leaf(
            "[system]\nname=\"t\"\n[image.z]\nboard=\"zephyr\"\n",
            &["src/a.c"],
        );
        assert_eq!(
            package_language(d.path()),
            Some(Language::Cpp),
            "a C leaf's Zephyr entry is the TYPED C++ carrier"
        );
        let (d, _) = leaf(
            "[system]\nname=\"t\"\n[image.z]\nboard=\"zephyr\"\n",
            &["Cargo.toml"],
        );
        assert_eq!(package_language(d.path()), Some(Language::Rust));
        let td = tempfile::tempdir().unwrap();
        assert_eq!(package_language(td.path()), None);
    }
}
