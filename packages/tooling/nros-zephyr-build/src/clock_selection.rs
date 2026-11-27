//! RFC-0141 D1/D2 (phase-582 W1d) — the image's `[knobs.clock]` selection,
//! as a build script that compiles a port's C clock code needs it.
//!
//! A port with no boot resolution point of its own (POSIX, FreeRTOS) resolves
//! the list in C, lazily, once, on the first wall-clock read; `board` gets the
//! settle window there, as `sntp` does on Zephyr. The C side learns the list
//! from defines (`nros/clock_resolve.h`'s `nros_clock_policy_from_build`), and
//! the `board` source from one extra translation unit, [`BOARD_TU`], compiled
//! only when `board` is selected. Every road that compiles the port's `platform.c` must apply the
//! SAME selection: two copies of the port's `platform.o` with different
//! defines would answer `nros_platform_time_now_ns` differently depending on
//! which archive the linker happened to scan first (issue 1779's demand-driven
//! duplicate). So the selection is read here, once, and each build script
//! applies [`ClockSelection::c_defines`] and [`BOARD_TU`] verbatim.

/// The selection-named TU, in the port's `src/` directory. It REFERENCES the
/// board hook `nros_board_wall_clock_ns` and defines none, so an image that
/// selects `board` without a hook fails to link, and the error names an
/// object that says which knob demanded it.
///
/// ONE file for every port, under `nros-platform-api/c/` ([`board_tu`]).
pub const BOARD_TU: &str = "nros_clock_board__set_by__knobs_clock_source.c";

/// [`BOARD_TU`]'s path, from the platform ABI's include dir
/// (`.../nros-platform-api/include`, which every port build already names).
#[must_use]
pub fn board_tu(api_include: &std::path::Path) -> std::path::PathBuf {
    api_include
        .parent()
        .unwrap_or(api_include)
        .join("c")
        .join(BOARD_TU)
}

/// What the image stated in `[knobs.clock]`, beyond the default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockSelection {
    /// `clock.source`, comma-joined (`"board,platform"`).
    pub sources: String,
    /// `clock.upgrade`, when stated.
    pub upgrade: Option<String>,
    /// `clock.epoch_failure`, when stated.
    pub epoch_failure: Option<String>,
    /// `clock.fallback_settle_ms`, when stated: how long each source (here
    /// `board`) gets to answer. Unstated, the C side takes the registry
    /// default (`NROS_CLOCK_FALLBACK_SETTLE_MS_DEFAULT`).
    pub settle_ms: Option<usize>,
    /// `clock.board_resync_interval_ms` and `clock.board_resync_stack_bytes`,
    /// when stated. Carried only when the list names `board`: no other source
    /// reads them.
    pub board_resync: BoardResync,
}

/// The `board` re-ask knobs, as stated (unstated: the C side's registry
/// defaults, interval 0 = asked once).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BoardResync {
    pub interval_ms: Option<usize>,
    pub stack_bytes: Option<usize>,
}

impl ClockSelection {
    /// Whether the list names `board`, so [`BOARD_TU`] must be compiled.
    #[must_use]
    pub fn board(&self) -> bool {
        self.sources.split(',').any(|s| s.trim() == "board")
    }

    /// The defines the port's C clock code reads, as `(name, value)` with
    /// string values already quoted as C literals.
    #[must_use]
    pub fn c_defines(&self) -> Vec<(&'static str, String)> {
        let mut out = vec![("NROS_CLOCK_SOURCE_CSV", format!("\"{}\"", self.sources))];
        if let Some(u) = &self.upgrade {
            out.push(("NROS_CLOCK_UPGRADE", format!("\"{u}\"")));
        }
        if let Some(e) = &self.epoch_failure {
            out.push(("NROS_CLOCK_EPOCH_FAILURE", format!("\"{e}\"")));
        }
        if let Some(ms) = self.settle_ms {
            out.push(("NROS_CLOCK_FALLBACK_SETTLE_MS", format!("{ms}u")));
        }
        if self.board() {
            out.push(("NROS_CLOCK_HAS_BOARD", "1".to_string()));
            if let Some(ms) = self.board_resync.interval_ms {
                out.push(("NROS_CLOCK_BOARD_RESYNC_INTERVAL_MS", format!("{ms}u")));
            }
            if let Some(b) = self.board_resync.stack_bytes {
                out.push(("NROS_CLOCK_BOARD_RESYNC_STACK_BYTES", format!("{b}u")));
            }
        }
        out
    }
}

/// The image's clock selection, or `None` when `clock.source` is unstated or
/// is the default `platform` alone: then the port compiles exactly what it
/// did before the knob existed. Reads through the one knob ladder
/// ([`crate::knob`]), which watches every env spelling it reads.
#[must_use]
pub fn clock_selection() -> Option<ClockSelection> {
    let sources = crate::knob("NROS_CLOCK_SOURCE").stated_choice()?;
    // Each knob is read through a LITERAL `crate::knob("NROS_...")`: the pool
    // inventory and the knob census find a knob's readers by that spelling.
    let settle_ms = in_range(
        crate::knob("NROS_CLOCK_FALLBACK_SETTLE_MS")
            .strict_env()
            .stated(),
        "NROS_CLOCK_FALLBACK_SETTLE_MS",
        "clock.fallback_settle_ms",
    );
    let board_resync = BoardResync {
        interval_ms: in_range(
            crate::knob("NROS_CLOCK_BOARD_RESYNC_INTERVAL_MS")
                .strict_env()
                .stated(),
            "NROS_CLOCK_BOARD_RESYNC_INTERVAL_MS",
            "clock.board_resync_interval_ms",
        ),
        stack_bytes: in_range(
            crate::knob("NROS_CLOCK_BOARD_RESYNC_STACK_BYTES")
                .strict_env()
                .stated(),
            "NROS_CLOCK_BOARD_RESYNC_STACK_BYTES",
            "clock.board_resync_stack_bytes",
        ),
    };
    from_stated(
        sources,
        crate::knob("NROS_CLOCK_UPGRADE").stated_choice(),
        crate::knob("NROS_CLOCK_EPOCH_FAILURE").stated_choice(),
        settle_ms,
        board_resync,
    )
}

/// A stated integer knob, refused outside its registry range.
fn in_range(v: Option<usize>, env: &str, key: &str) -> Option<usize> {
    if let (Some(n), Some((lo, hi))) = (v, crate::knob_row(env).and_then(|r| r.range))
        && ((n as i64) < lo || n as i64 > hi)
    {
        panic!("nros-zephyr-build: {key} = {n} is outside its range ({lo}..={hi})");
    }
    v
}

/// Pure core of [`clock_selection`].
fn from_stated(
    sources: String,
    upgrade: Option<String>,
    epoch_failure: Option<String>,
    settle_ms: Option<usize>,
    board_resync: BoardResync,
) -> Option<ClockSelection> {
    let normalised: Vec<&str> = sources.split(',').map(str::trim).collect();
    if normalised == ["platform"] {
        return None;
    }
    Some(ClockSelection {
        sources: normalised.join(","),
        upgrade,
        epoch_failure,
        settle_ms,
        board_resync,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared table both readers answer to (`tests/data/clock-selection-cases.tsv`;
    /// the CMake reader's twin is `tests/clock-selection-cmake-parity.sh`).
    #[test]
    fn the_shared_case_table() {
        let table = include_str!("../../../../tests/data/clock-selection-cases.tsv");
        let opt = |s: &str| (s != "-").then(|| s.to_string());
        let mut rows = 0;
        for line in table
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        {
            let f: Vec<&str> = line.split('\t').collect();
            assert_eq!(f.len(), 7, "malformed row: {line:?}");
            let got = from_stated(
                f[0].to_string(),
                opt(f[1]),
                opt(f[2]),
                opt(f[3]).map(|s| s.parse().expect("settle ms")),
                BoardResync {
                    interval_ms: opt(f[4]).map(|s| s.parse().expect("resync ms")),
                    stack_bytes: opt(f[5]).map(|s| s.parse().expect("stack bytes")),
                },
            )
            .map_or_else(
                || "NONE".to_string(),
                |s| {
                    s.c_defines()
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(";")
                },
            );
            assert_eq!(got, f[6], "row {line:?}");
            rows += 1;
        }
        assert!(rows >= 5, "the table lost its rows ({rows})");
    }

    #[test]
    fn the_default_selects_nothing() {
        assert_eq!(
            from_stated("platform".into(), None, None, None, BoardResync::default()),
            None
        );
        assert_eq!(
            from_stated(
                " platform ".into(),
                Some("monotonic".into()),
                None,
                Some(5),
                BoardResync::default()
            ),
            None
        );
    }

    #[test]
    fn board_adds_its_define_and_quotes_strings() {
        let s = from_stated(
            "board, platform".into(),
            None,
            Some("halt".into()),
            Some(300),
            BoardResync {
                interval_ms: Some(1000),
                stack_bytes: None,
            },
        )
        .unwrap();
        assert!(s.board());
        assert_eq!(
            s.c_defines(),
            vec![
                ("NROS_CLOCK_SOURCE_CSV", "\"board,platform\"".to_string()),
                ("NROS_CLOCK_EPOCH_FAILURE", "\"halt\"".to_string()),
                ("NROS_CLOCK_FALLBACK_SETTLE_MS", "300u".to_string()),
                ("NROS_CLOCK_HAS_BOARD", "1".to_string()),
                ("NROS_CLOCK_BOARD_RESYNC_INTERVAL_MS", "1000u".to_string()),
            ]
        );
    }

    /// The board re-ask knobs reach C only beside `board`: no other source
    /// reads them, and a define nobody reads is a second source of truth.
    #[test]
    fn board_resync_is_carried_only_with_board() {
        let r = BoardResync {
            interval_ms: Some(500),
            stack_bytes: Some(4096),
        };
        let s = from_stated("sntp,platform".into(), None, None, None, r).unwrap();
        assert!(
            s.c_defines()
                .iter()
                .all(|(k, _)| !k.starts_with("NROS_CLOCK_BOARD_RESYNC"))
        );
    }

    /// The C side's defaults for the unstated board re-ask knobs are the
    /// registry's.
    #[test]
    fn the_c_board_resync_defaults_are_the_registry_defaults() {
        let tu = include_str!(
            "../../../platform/nros-platform-api/c/nros_clock_board__set_by__knobs_clock_source.c"
        );
        for env in [
            "NROS_CLOCK_BOARD_RESYNC_INTERVAL_MS",
            "NROS_CLOCK_BOARD_RESYNC_STACK_BYTES",
        ] {
            let row = crate::knob_row(env).expect("registry row");
            let crate::KnobDefault::Kconfig(defaults) = row.default else {
                panic!("{env} has no Kconfig default");
            };
            let registry: u64 = defaults
                .last()
                .expect("a default line")
                .value
                .parse()
                .expect("a number");
            let prefix = format!("#define {env} ");
            let c: u64 = tu
                .lines()
                .find_map(|l| l.trim().strip_prefix(prefix.as_str()))
                .unwrap_or_else(|| panic!("the board TU defines no default for {env}"))
                .trim()
                .trim_end_matches('u')
                .parse()
                .expect("a number");
            assert_eq!(
                c, registry,
                "{env}: the board TU's default drifted from knobs.toml"
            );
        }
    }

    #[test]
    fn a_list_without_board_has_no_board_define() {
        let s = from_stated(
            "sntp,platform".into(),
            None,
            None,
            None,
            BoardResync::default(),
        )
        .unwrap();
        assert!(!s.board());
        assert!(
            s.c_defines()
                .iter()
                .all(|(k, _)| *k != "NROS_CLOCK_HAS_BOARD")
        );
    }

    #[test]
    fn an_unstated_settle_window_leaves_the_c_default() {
        let s = from_stated("board".into(), None, None, None, BoardResync::default()).unwrap();
        assert!(
            s.c_defines()
                .iter()
                .all(|(k, _)| *k != "NROS_CLOCK_FALLBACK_SETTLE_MS")
        );
    }

    /// The C side's default for an unstated `clock.fallback_settle_ms` is the
    /// registry's, so an image that states nothing waits as long on every road.
    #[test]
    fn the_c_settle_default_is_the_registry_default() {
        let row = crate::knob_row("NROS_CLOCK_FALLBACK_SETTLE_MS").expect("registry row");
        let crate::KnobDefault::Kconfig(defaults) = row.default else {
            panic!("clock.fallback_settle_ms has no Kconfig default");
        };
        let registry: u64 = defaults
            .last()
            .expect("a default line")
            .value
            .parse()
            .expect("a number");
        let header =
            include_str!("../../../platform/nros-platform-api/include/nros/clock_resolve.h");
        let line = header
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix("#define NROS_CLOCK_FALLBACK_SETTLE_MS_DEFAULT ")
            })
            .expect("clock_resolve.h defines NROS_CLOCK_FALLBACK_SETTLE_MS_DEFAULT");
        let c: u64 = line.trim().trim_end_matches('u').parse().expect("a number");
        assert_eq!(
            c, registry,
            "clock_resolve.h's settle default drifted from knobs.toml"
        );
    }

    #[test]
    fn the_board_tu_is_beside_the_api_include() {
        let p = board_tu(std::path::Path::new("/x/nros-platform-api/include"));
        assert_eq!(
            p,
            std::path::Path::new("/x/nros-platform-api/c").join(BOARD_TU)
        );
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../platform/nros-platform-api/include");
        assert!(
            board_tu(&repo).is_file(),
            "{} is missing",
            board_tu(&repo).display()
        );
    }
}
