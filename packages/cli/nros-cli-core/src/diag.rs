//! The CLI's constructors for diagnostic codes (RFC-0129 D2).
//!
//! A user-reachable refusal is built from its entry in
//! `nros-diagnostic-codes`, never written as free text:
//!
//! ```ignore
//! use crate::diag::{Diag, DiagExt};
//! return Err(Diag::BoardUnknown.err(format_args!("board `{b}` is claimed by no board descriptor")));
//! ```
//!
//! renders
//!
//! ```text
//! error[NROS-0002]: board `acme` is claimed by no board descriptor
//!   help: check the spelling of the image's `board`; `nros board list` names every board this SDK has
//!   explain: nros explain NROS-0002
//! ```
//!
//! A site with a more specific remedy uses [`DiagExt::refusal`] and
//! [`Refusal::help`]; a contributor's note (ids allowed) goes in
//! [`Refusal::note`] and prints only under `-v` / `NROS_VERBOSE=1`. A site
//! whose error type is a `String` takes the rendered text with `to_string()`.

use std::fmt;

pub use nros_diagnostic_codes::{ALL, Diag, Entry};

/// An owned refusal: what the CLI returns. Renders RFC-0129 D2's lines.
#[derive(Debug, Clone)]
pub struct Refusal {
    diag: Diag,
    detail: String,
    help: Option<String>,
    note: Option<String>,
}

impl Refusal {
    /// Replace the entry's remedy with a more specific one.
    #[must_use]
    pub fn help(mut self, help: impl fmt::Display) -> Self {
        self.help = Some(help.to_string());
        self
    }

    /// A contributor's note, printed only under verbose output. The one place
    /// a refusal may carry an internal id.
    #[must_use]
    pub fn note(mut self, note: impl fmt::Display) -> Self {
        self.note = Some(note.to_string());
        self
    }

    /// The diagnostic this refusal raises.
    #[must_use]
    pub fn diag(&self) -> Diag {
        self.diag
    }

    /// As an `eyre::Report`.
    pub fn report(self) -> eyre::Report {
        eyre::Report::new(self)
    }

    /// Render with an explicit verbosity: the tests' seam.
    fn render(&self, f: &mut fmt::Formatter<'_>, verbose: bool) -> fmt::Result {
        let mut r = self.diag.refuse(&self.detail).verbose(verbose);
        if let Some(h) = &self.help {
            r = r.help(h);
        }
        if let Some(n) = &self.note {
            r = r.note(n);
        }
        fmt::Display::fmt(&r, f)
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.render(f, crate::build_notes::verbose())
    }
}

impl std::error::Error for Refusal {}

/// The CLI's constructors on [`Diag`].
pub trait DiagExt {
    /// The refusal as an `eyre::Report`, with `detail` as the site's words.
    fn err(self, detail: fmt::Arguments<'_>) -> eyre::Report;
    /// The refusal, to add a [`Refusal::help`] or [`Refusal::note`] to.
    fn refusal(self, detail: impl fmt::Display) -> Refusal;
}

impl DiagExt for Diag {
    fn err(self, detail: fmt::Arguments<'_>) -> eyre::Report {
        self.refusal(detail).report()
    }

    fn refusal(self, detail: impl fmt::Display) -> Refusal {
        Refusal {
            diag: self,
            detail: detail.to_string(),
            help: None,
            note: None,
        }
    }
}

/// Whether `text` is a rendered refusal: it starts `error[NROS-NNNN]: `.
#[must_use]
pub fn is_rendered(text: &str) -> bool {
    head_len(text).is_some()
}

/// The length of the `error[NROS-NNNN]: ` head, if `text` starts with one.
fn head_len(text: &str) -> Option<usize> {
    let rest = text.strip_prefix("error[")?;
    let code = rest.get(..9)?;
    nros_diagnostic_codes::parse_code(code)?;
    rest[9..]
        .starts_with("]: ")
        .then_some("error[".len() + 9 + "]: ".len())
}

/// Put `context` in front of a refusal's detail: `error[NROS-0005]: <context><detail>`.
/// A site that adds context to a message it did not build uses this, so the
/// code stays at the head of the line. Text that is not a rendered refusal
/// gets `context` in front of it as before.
#[must_use]
pub fn prefix_detail(context: &str, text: &str) -> String {
    match head_len(text) {
        Some(n) => format!("{}{context}{}", &text[..n], &text[n..]),
        None => format!("{context}{text}"),
    }
}

/// Whether `main` prints this error as it is, with no `Error: ` in front: a
/// rendered refusal that nothing wrapped. A refusal under added context keeps
/// eyre's report, so the context still prints.
#[must_use]
pub fn prints_bare(e: &eyre::Report) -> bool {
    e.chain().count() == 1 && is_rendered(&e.to_string())
}

/// What `nros explain NROS-NNNN` prints, or the refusal for a code no entry
/// has (with the three nearest codes).
pub fn explain(token: &str) -> eyre::Result<String> {
    if let Some(e) = nros_diagnostic_codes::lookup(token) {
        return Ok(nros_diagnostic_codes::views::explain_text(e));
    }
    let n = nros_diagnostic_codes::parse_code(token).unwrap_or(0);
    let near: Vec<&str> = nros_diagnostic_codes::nearest(n)
        .iter()
        .flatten()
        .map(|e| e.code)
        .collect();
    Err(Diag::UnknownDiagCode
        .refusal(format_args!("no diagnostic has the code `{token}`"))
        .help(format_args!(
            "the nearest codes are {}; check the code against the refusal that printed it",
            near.join(", ")
        ))
        .report())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rendered with verbosity fixed, whatever the test process inherited.
    fn quiet(r: &Refusal) -> String {
        struct Q<'a>(&'a Refusal, bool);
        impl fmt::Display for Q<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.render(f, self.1)
            }
        }
        Q(r, false).to_string()
    }

    fn loud(r: &Refusal) -> String {
        struct Q<'a>(&'a Refusal);
        impl fmt::Display for Q<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.render(f, true)
            }
        }
        Q(r).to_string()
    }

    /// `check-user-surface-ids`' `ID` pattern, verbatim.
    fn ids(text: &str) -> Vec<String> {
        let re = regex::Regex::new(
            r"\b[Pp]hase[- ]\d{2,4}\b|\b[Ii]ssues?[- ]#?\d{3,4}\b|(?:^|[^\w&/#])#\d{3,4}\b|\bRFC-?\d{4}\b",
        )
        .unwrap();
        // The gate's `#NNN` arm is a lookbehind; this one consumes the
        // character before the `#`, so drop it.
        re.find_iter(text)
            .map(|m| {
                let s = m.as_str();
                match s.find('#') {
                    Some(i) if !s.starts_with(['I', 'i']) => s[i..].to_string(),
                    _ => s.to_string(),
                }
            })
            .collect()
    }

    #[test]
    fn err_renders_detail_remedy_and_explain_line() {
        let b = "mps2-an358";
        let e = Diag::BoardUnknown.err(format_args!(
            "board `{b}` is claimed by no board descriptor"
        ));
        let r = e.downcast_ref::<Refusal>().expect("a Refusal");
        assert_eq!(
            quiet(r),
            "error[NROS-0002]: board `mps2-an358` is claimed by no board descriptor\n  \
             help: check the spelling of the image's `board`; `nros board list` names every \
             board this SDK has\n  \
             explain: nros explain NROS-0002"
        );
        assert!(ids(&quiet(r)).is_empty());
    }

    #[test]
    fn help_overrides_the_remedy_and_note_needs_verbose() {
        let r = Diag::ImageNoBoard
            .refusal("s.toml: names no board")
            .help("add `board = \"linux\"`")
            .note("RFC-0098 D3");
        assert_eq!(
            quiet(&r),
            "error[NROS-0004]: s.toml: names no board\n  \
             help: add `board = \"linux\"`\n  \
             explain: nros explain NROS-0004"
        );
        assert!(ids(&quiet(&r)).is_empty());
        assert_eq!(
            loud(&r),
            "error[NROS-0004]: s.toml: names no board\n  \
             help: add `board = \"linux\"`\n  \
             note: RFC-0098 D3\n  \
             explain: nros explain NROS-0004"
        );
    }

    #[test]
    fn the_id_pattern_matches_what_the_gate_matches() {
        assert_eq!(
            ids("phase-552 issue 2305 RFC-0129 PR #472"),
            ["phase-552", "issue 2305", "RFC-0129", "#472"]
        );
        assert!(ids("error[NROS-0412]: nros explain NROS-0412").is_empty());
    }

    #[test]
    fn context_goes_after_the_code() {
        let text = Diag::PlatformNoFamily.refusal("platform `x` …").to_string();
        let ctx = prefix_detail("board acme: ", &text);
        assert!(
            ctx.starts_with("error[NROS-0005]: board acme: platform `x` …\n"),
            "{ctx}"
        );
        assert_eq!(prefix_detail("a: ", "plain"), "a: plain");
        assert!(!is_rendered("error[NROS-05]: x"));
    }

    #[test]
    fn main_prints_an_unwrapped_refusal_bare() {
        use eyre::WrapErr;
        let e = Diag::NoImageDeclared.err(format_args!("x"));
        assert!(prints_bare(&e));
        let s = eyre::eyre!("{}", Diag::NoImageDeclared.refusal("x"));
        assert!(prints_bare(&s), "a rendered String counts too");
        let wrapped: eyre::Result<()> = Err(Diag::NoImageDeclared.err(format_args!("x")));
        let wrapped = wrapped.wrap_err("while planning").unwrap_err();
        assert!(!prints_bare(&wrapped));
        assert!(!prints_bare(&eyre::eyre!("plain")));
    }

    #[test]
    fn explain_prints_a_live_code_and_refuses_an_unknown_one() {
        let t = explain("NROS-0001").unwrap();
        assert!(t.starts_with("NROS-0001: ROS 2 is not sourced"), "{t}");
        assert!(ids(&t).is_empty(), "{t}");
        let e = explain("NROS-0099").unwrap_err();
        let r = e.downcast_ref::<Refusal>().unwrap();
        assert_eq!(
            quiet(r),
            "error[NROS-0006]: no diagnostic has the code `NROS-0099`\n  \
             help: the nearest codes are NROS-0006, NROS-0005, NROS-0004; check the code \
             against the refusal that printed it\n  \
             explain: nros explain NROS-0006"
        );
    }
}
