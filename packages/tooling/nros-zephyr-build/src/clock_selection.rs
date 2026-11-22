//! RFC-0141 D1/D2 (phase-582 W1d) — the image's `[knobs.clock]` selection,
//! as a build script that compiles a port's C clock code needs it.
//!
//! A port whose sources all answer synchronously (the POSIX port: `board`,
//! `platform`) resolves the list in C, lazily, on the first wall-clock read.
//! The C side learns the list from defines, and the `board` source from one
//! extra translation unit, [`BOARD_TU`], compiled only when `board` is
//! selected. Every road that compiles the port's `platform.c` must apply the
//! SAME selection: two copies of the port's `platform.o` with different
//! defines would answer `nros_platform_time_now_ns` differently depending on
//! which archive the linker happened to scan first (issue 1779's demand-driven
//! duplicate). So the selection is read here, once, and each build script
//! applies [`ClockSelection::c_defines`] and [`BOARD_TU`] verbatim.

/// The selection-named TU, in the port's `src/` directory. It REFERENCES the
/// board hook `nros_board_wall_clock_ns` and defines none, so an image that
/// selects `board` without a hook fails to link, and the error names an
/// object that says which knob demanded it.
pub const BOARD_TU: &str = "nros_clock_board__set_by__knobs_clock_source.c";

/// What the image stated in `[knobs.clock]`, beyond the default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockSelection {
    /// `clock.source`, comma-joined (`"board,platform"`).
    pub sources: String,
    /// `clock.upgrade`, when stated.
    pub upgrade: Option<String>,
    /// `clock.epoch_failure`, when stated.
    pub epoch_failure: Option<String>,
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
        if self.board() {
            out.push(("NROS_CLOCK_HAS_BOARD", "1".to_string()));
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
    from_stated(
        sources,
        crate::knob("NROS_CLOCK_UPGRADE").stated_choice(),
        crate::knob("NROS_CLOCK_EPOCH_FAILURE").stated_choice(),
    )
}

/// Pure core of [`clock_selection`].
fn from_stated(
    sources: String,
    upgrade: Option<String>,
    epoch_failure: Option<String>,
) -> Option<ClockSelection> {
    let normalised: Vec<&str> = sources.split(',').map(str::trim).collect();
    if normalised == ["platform"] {
        return None;
    }
    Some(ClockSelection {
        sources: normalised.join(","),
        upgrade,
        epoch_failure,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_selects_nothing() {
        assert_eq!(from_stated("platform".into(), None, None), None);
        assert_eq!(
            from_stated(" platform ".into(), Some("monotonic".into()), None),
            None
        );
    }

    #[test]
    fn board_adds_its_define_and_quotes_strings() {
        let s = from_stated("board, platform".into(), None, Some("halt".into())).unwrap();
        assert!(s.board());
        assert_eq!(
            s.c_defines(),
            vec![
                ("NROS_CLOCK_SOURCE_CSV", "\"board,platform\"".to_string()),
                ("NROS_CLOCK_EPOCH_FAILURE", "\"halt\"".to_string()),
                ("NROS_CLOCK_HAS_BOARD", "1".to_string()),
            ]
        );
    }

    #[test]
    fn a_list_without_board_has_no_board_define() {
        let s = from_stated("sntp,platform".into(), None, None).unwrap();
        assert!(!s.board());
        assert!(
            s.c_defines()
                .iter()
                .all(|(k, _)| *k != "NROS_CLOCK_HAS_BOARD")
        );
    }
}
