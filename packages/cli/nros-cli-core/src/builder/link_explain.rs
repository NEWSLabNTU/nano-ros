//! After a FAILED build, read the native tool's log and say what the build
//! already knows about a memory-region overflow at link (phase-588 W3).
//!
//! The linker's line is all a user gets otherwise:
//!
//! ```text
//! region `RAM' overflowed by 7680 bytes
//! ```
//!
//! It does not say that the image's main stack is the largest thing in that
//! region, where that number came from, or which knob is the user's to move.
//! The build knows all three. This module is the pure half: it finds the
//! overflow in the log text and renders the explanation from facts the
//! caller gathered. It reads no file and prints nothing, so it is tested on
//! strings.
//!
//! What it never says: turn the stack-floor check off. That is not a fix, and
//! the check stays on by default. When the floor is an upper bound, it says so
//! in those words.

use std::fmt::Write as _;

/// One `region <R> overflowed by <N> bytes` the linker reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overflow {
    /// The memory region, as the linker script names it (`RAM`, `FLASH`, `DTCM`).
    pub region: String,
    /// How many bytes it is short.
    pub bytes: u64,
}

/// Every region overflow in a build log, first occurrence of each region
/// only, in the order the linker reported them.
///
/// Two spellings:
/// - GNU ld: ``region `RAM' overflowed by 7680 bytes``
/// - LLD: `section '.bss' will not fit in region 'RAM': overflowed by 7680 bytes`
#[must_use]
pub fn overflows(log: &str) -> Vec<Overflow> {
    let mut out: Vec<Overflow> = Vec::new();
    for line in log.lines() {
        let Some(o) = parse_line(line) else { continue };
        if !out.iter().any(|e| e.region == o.region) {
            out.push(o);
        }
    }
    out
}

fn parse_line(line: &str) -> Option<Overflow> {
    // Both spellings put the region right after "region " and end with
    // "overflowed by <N> bytes".
    let at = line.find("region ")?;
    let rest = &line[at + "region ".len()..];
    let quote = rest.chars().next()?;
    if !matches!(quote, '`' | '\'' | '"') {
        return None;
    }
    let body = &rest[quote.len_utf8()..];
    let end = body.find(['\'', '`', '"'])?;
    let region = body[..end].to_string();
    let tail = &body[end..];
    let by = tail.find("overflowed by ")?;
    let n: String = tail[by + "overflowed by ".len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let bytes = n.parse().ok()?;
    (!region.is_empty()).then_some(Overflow { region, bytes })
}

/// The `config/stack-floors.toml` row an image is measured under
/// (`[image.<id>] stack_floor_image`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FloorRow {
    pub key: String,
    /// The static walk's bytes.
    pub static_bytes: u64,
    /// The measured peak, when the row has one.
    pub peak: Option<u64>,
    /// The table's margin, in percent.
    pub margin_pct: u32,
    /// The walk crosses an indirect call bound only by type, so its number
    /// is an upper bound, not a size.
    pub upper_bound: bool,
}

/// What the build knows about the image whose link failed. Each is a fact the
/// build read; none is inferred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageMemoryFacts {
    /// The image, as `nros build` names it.
    pub image: String,
    /// The main stack the link used, and the symbol it was read from.
    pub main_stack: Option<(u64, String)>,
    /// The stack-floor row the image is measured under.
    pub floor_row: Option<FloorRow>,
    /// The heap the link used, and the knob that sizes it.
    pub heap: Option<(u64, String)>,
    /// Where the user states the main stack for this image, the way out when
    /// the stack is the thing to move.
    pub stack_knob: String,
}

/// `12345` as `12,345`.
fn bytes(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The explanation for a failed build's log, or `None` when the log holds no
/// region overflow (so the caller prints nothing new).
#[must_use]
pub fn explain(log: &str, facts: &ImageMemoryFacts) -> Option<String> {
    let found = overflows(log);
    if found.is_empty() {
        return None;
    }
    let mut t = String::new();
    let regions: Vec<String> = found
        .iter()
        .map(|o| format!("`{}` by {} B", o.region, bytes(o.bytes)))
        .collect();
    let _ = writeln!(
        t,
        "nros build: image `{}` does not fit its memory: the link overflowed {}.",
        facts.image,
        regions.join(" and ")
    );
    if facts.main_stack.is_some() || facts.heap.is_some() {
        let _ = writeln!(t, "  What the image reserves:");
    }
    if let Some((n, from)) = &facts.main_stack {
        let _ = writeln!(t, "    main stack  {} B, {from}", bytes(*n));
    }
    if let Some(r) = &facts.floor_row {
        let peak = r
            .peak
            .map_or(String::new(), |p| format!(", measured peak {} B", bytes(p)));
        let bound = if r.upper_bound {
            " (an upper bound through an indirect call, not a size)"
        } else {
            ""
        };
        let _ = writeln!(
            t,
            "      measured under the stack-floor row `{}`: static walk {} B{bound}{peak}, \
             margin {} %",
            r.key,
            bytes(r.static_bytes),
            r.margin_pct
        );
    }
    if let Some((n, knob)) = &facts.heap {
        let _ = writeln!(t, "    heap        {} B, {knob}", bytes(*n));
    }
    let _ = writeln!(
        t,
        "  To make room, lower what the image adds (a pool or cache knob it raised), \
         the heap, or the main stack: {}.",
        facts.stack_knob
    );
    Some(t)
}

/// What the build knows about `plan`'s memory, read after its link failed.
///
/// Everything here is read, never inferred:
/// - for a west image, the main stack and the nros heap the link used, from
///   the build's own `zephyr/.config`;
/// - the stack-floor row the image is measured under, from the nano-ros
///   tree's `config/stack-floors.toml`.
///
/// A fact that cannot be read is left out, and its line with it.
#[must_use]
pub fn facts_for(plan: &crate::cmd::build::ResolvedBuild) -> ImageMemoryFacts {
    let id = plan
        .qualified
        .rsplit(':')
        .next()
        .unwrap_or(&plan.qualified)
        .to_string();
    let mut f = ImageMemoryFacts {
        image: plan.qualified.clone(),
        main_stack: None,
        floor_row: None,
        heap: None,
        stack_knob: format!("`[image.{id}.knobs.memory] app_stack_bytes`"),
    };
    if let Some(crate::cmd::build_next::BuildOutput::West {
        build_dir: Some(dir),
    }) = &plan.output
    {
        if let Ok(conf) = std::fs::read_to_string(dir.join("zephyr").join(".config")) {
            f.main_stack = kconfig_int(&conf, "CONFIG_MAIN_STACK_SIZE")
                .map(|v| (v, "CONFIG_MAIN_STACK_SIZE".to_string()));
            f.heap = kconfig_int(&conf, "CONFIG_NROS_MEMORY_HEAP_BYTES")
                .map(|v| (v, format!("`[image.{id}.knobs.memory] heap_bytes`")));
        }
        f.stack_knob =
            format!("`CONFIG_MAIN_STACK_SIZE` in a conf file `[image.{id}] os_config` names");
    }
    if let Some(key) = &plan.stack_floor_image {
        let root = std::env::current_dir()
            .ok()
            .and_then(|cwd| crate::orchestration::nano_ros_root::resolve(None, &cwd));
        f.floor_row = root
            .and_then(|r| std::fs::read_to_string(r.join("config/stack-floors.toml")).ok())
            .and_then(|t| floor_row(&t, key));
    }
    f
}

/// `KEY=<integer>` from a Kconfig `.config`; a negative value (Kconfig's
/// "unset") reads as absent.
fn kconfig_int(conf: &str, key: &str) -> Option<u64> {
    conf.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .and_then(|v| v.trim().parse::<i64>().ok())
        .and_then(|v| u64::try_from(v).ok())
}

/// `key`'s OPEN row in a stack-floor table: an `[[observed]]` row (static,
/// peak, upper bound) or an `[[allowance]]` row (its `measured.static`).
#[must_use]
pub fn floor_row(table: &str, key: &str) -> Option<FloorRow> {
    let doc: toml::Value = toml::from_str(table).ok()?;
    let margin_pct = u32::try_from(doc.get("margin_pct")?.as_integer()?).ok()?;
    let open = |name: &str| {
        doc.get(name)
            .and_then(toml::Value::as_array)
            .and_then(|rows| {
                rows.iter()
                    .find(|r| {
                        r.get("image").and_then(toml::Value::as_str) == Some(key)
                            && r.get("role").and_then(toml::Value::as_str) == Some("OPEN")
                    })
                    .cloned()
            })
    };
    let int = |v: Option<&toml::Value>| {
        v.and_then(toml::Value::as_integer)
            .and_then(|n| u64::try_from(n).ok())
    };
    if let Some(r) = open("observed") {
        return Some(FloorRow {
            key: key.to_string(),
            static_bytes: int(r.get("static"))?,
            peak: int(r.get("peak")),
            margin_pct,
            upper_bound: r.get("upper_bound").is_some(),
        });
    }
    let r = open("allowance")?;
    let m = r.get("measured")?;
    Some(FloorRow {
        key: key.to_string(),
        static_bytes: int(m.get("static"))?,
        peak: int(m.get("peak")),
        margin_pct,
        upper_bound: r.get("upper_bound").is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> ImageMemoryFacts {
        ImageMemoryFacts {
            image: "tlk_node_bringup:zephyr".into(),
            main_stack: Some((75_536, "CONFIG_MAIN_STACK_SIZE".into())),
            floor_row: Some(FloorRow {
                key: "talker/rust/zenoh@nucleo-f767zi-zephyr".into(),
                static_bytes: 69_648,
                peak: Some(28_888),
                margin_pct: 10,
                upper_bound: true,
            }),
            heap: Some((196_608, "[image.zephyr.knobs.memory] heap_bytes".into())),
            stack_knob: "`CONFIG_MAIN_STACK_SIZE` in a conf file the image's `os_config` names"
                .into(),
        }
    }

    #[test]
    fn gnu_ld_and_lld_spellings_are_both_read() {
        let log = "\
[412/415] Linking C executable zephyr/zephyr_pre0.elf
/opt/zephyr-sdk/arm-zephyr-eabi/bin/ld: zephyr/zephyr_pre0.elf section `noinit' will not fit in region `RAM'
/opt/zephyr-sdk/arm-zephyr-eabi/bin/ld: region `RAM' overflowed by 7680 bytes
ld.lld: error: section '.bss' will not fit in region 'DTCM': overflowed by 1024 bytes
ld.lld: error: section '.data' will not fit in region 'DTCM': overflowed by 2048 bytes
";
        assert_eq!(
            overflows(log),
            [
                Overflow {
                    region: "RAM".into(),
                    bytes: 7680
                },
                Overflow {
                    region: "DTCM".into(),
                    bytes: 1024
                },
            ],
            "one entry per region, first report wins"
        );
    }

    #[test]
    fn an_overflow_is_explained_from_what_the_build_knows() {
        let t = explain("ld: region `RAM' overflowed by 7680 bytes\n", &facts()).unwrap();
        for want in [
            "image `tlk_node_bringup:zephyr`",
            "`RAM` by 7,680 B",
            "main stack  75,536 B, CONFIG_MAIN_STACK_SIZE",
            "talker/rust/zenoh@nucleo-f767zi-zephyr",
            "measured peak 28,888 B",
            "an upper bound",
            "heap        196,608 B, [image.zephyr.knobs.memory] heap_bytes",
            "a conf file the image's `os_config` names",
        ] {
            assert!(t.contains(want), "missing {want:?} in:\n{t}");
        }
        // The check is not the fix.
        assert!(!t.contains("NROS_STACK_FLOOR"), "{t}");
        assert!(!t.to_lowercase().contains("off"), "{t}");
    }

    #[test]
    fn a_log_without_an_overflow_explains_nothing() {
        let log = "error[E0308]: mismatched types\nerror: could not compile `x`\n";
        assert_eq!(explain(log, &facts()), None);
        assert_eq!(explain("", &facts()), None);
    }

    #[test]
    fn missing_facts_leave_their_lines_out() {
        let f = ImageMemoryFacts {
            main_stack: None,
            floor_row: None,
            heap: None,
            ..facts()
        };
        let t = explain("region `FLASH' overflowed by 12 bytes", &f).unwrap();
        assert!(t.contains("`FLASH` by 12 B"), "{t}");
        assert!(!t.contains("reserves"), "{t}");
        assert!(!t.contains("stack-floor row"), "{t}");
        assert!(t.contains("os_config"), "{t}");
    }

    #[test]
    fn the_floor_row_and_the_link_s_numbers_are_read_not_inferred() {
        let table = r#"
margin_pct = 10
drift_rise_pct = 2

[[observed]]
image = "talker/rust/zenoh@nucleo-f767zi-zephyr"
role = "OPEN"
peak = 28888
static = 69648
upper_bound = "a -> b (type)"

[[observed]]
image = "talker/rust/zenoh@nucleo-f767zi-zephyr"
role = "CLOSED"
static = 1
"#;
        assert_eq!(
            floor_row(table, "talker/rust/zenoh@nucleo-f767zi-zephyr"),
            Some(FloorRow {
                key: "talker/rust/zenoh@nucleo-f767zi-zephyr".into(),
                static_bytes: 69_648,
                peak: Some(28_888),
                margin_pct: 10,
                upper_bound: true,
            })
        );
        assert_eq!(floor_row(table, "no/such@row"), None);
        let conf = "CONFIG_MAIN_STACK_SIZE=33288\nCONFIG_TEST_UNSET_VALUE=-1\n";
        assert_eq!(kconfig_int(conf, "CONFIG_MAIN_STACK_SIZE"), Some(33_288));
        assert_eq!(
            kconfig_int(conf, "CONFIG_TEST_UNSET_VALUE"),
            None,
            "-1 is unset"
        );
        assert_eq!(
            kconfig_int(conf, "CONFIG_MAIN_STACK"),
            None,
            "a prefix is not the key"
        );
    }

    /// The real table names a row for every template the floor check reads.
    #[test]
    fn the_shipped_table_reads_for_the_stock_talker() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let table = std::fs::read_to_string(root.join("config/stack-floors.toml")).unwrap();
        let r = floor_row(&table, "talker/rust/zenoh@nucleo-f767zi-zephyr").unwrap();
        assert!(r.static_bytes > 0 && r.margin_pct > 0, "{r:?}");
    }

    #[test]
    fn thousands_are_grouped() {
        assert_eq!(bytes(0), "0");
        assert_eq!(bytes(999), "999");
        assert_eq!(bytes(1000), "1,000");
        assert_eq!(bytes(1_234_567), "1,234,567");
    }
}
