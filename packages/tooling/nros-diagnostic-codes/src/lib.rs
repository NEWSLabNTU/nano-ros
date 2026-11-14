//! The one list of nano-ros diagnostic codes (RFC-0129).
//!
//! Every refusal a user can meet is declared once, in [`codes`], through the
//! [`diagnostics!`] macro. A declaration yields:
//!
//! * a variant of [`Diag`], whose discriminant is the code's number, so a
//!   refusal is constructed from its entry and never written as free text;
//! * a row of [`ALL`], holding the code, name, title, surfaces, remedy and
//!   explanation;
//! * the explanation itself, read with `include_str!` from
//!   `explain/NROS-NNNN.md`, so a code without an explanation does not compile.
//!
//! A retired code keeps its row and its number, points at its successor, and
//! has no [`Diag`] variant, so nothing can construct it.
//!
//! Every surface renders the same three lines ([`Refusal`]):
//!
//! ```text
//! error[NROS-0001]: <the site's own words>
//!   help: <the entry's remedy, or the site's more specific one>
//!   explain: nros explain NROS-0001
//! ```
//!
//! No rendered line names an issue, phase or RFC id. A contributor's id goes
//! in a source comment, or in a [`Refusal::note`], which prints only under
//! verbose output.

#![no_std]

#[cfg(any(feature = "std", test))]
extern crate std;

use core::fmt;

#[macro_use]
mod macros;
mod codes;

#[cfg(any(feature = "std", test))]
pub mod views;

pub use codes::{ALL, Diag};

/// The founding range, `NROS-0001..=NROS-{FOUNDING_MAX}`: the codes allocated
/// in the founding commit, before codes were claimed on origin. They have no
/// `refs/diag-codes/NNNN` claim, and the pre-push guard exempts exactly this
/// range (it reads it from `codes.lock`'s `# founding:` line). It never grows:
/// `check-diagnostics` holds it to the baseline's.
pub const FOUNDING_MAX: u16 = 6;

/// Where a diagnostic can be raised. Every entry names at least one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    /// The `nros` command line.
    Cli,
    /// A CMake configure.
    Cmake,
    /// A Rust build script.
    BuildScript,
    /// A running image's log.
    Runtime,
    /// The colcon plugin.
    Colcon,
}

impl Surface {
    /// The surface's name as the macro spells it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Surface::Cli => "Cli",
            Surface::Cmake => "Cmake",
            Surface::BuildScript => "BuildScript",
            Surface::Runtime => "Runtime",
            Surface::Colcon => "Colcon",
        }
    }
}

/// One row of the list: a live code, or a retired one.
#[derive(Debug)]
pub struct Entry {
    /// The number, `1..=9999`.
    pub number: u16,
    /// The code as printed, `NROS-NNNN`.
    pub code: &'static str,
    /// The name the code was declared with.
    pub name: &'static str,
    /// One line saying what went wrong. Empty for a retired code.
    pub title: &'static str,
    /// Where the code can be raised. Empty for a retired code.
    pub surfaces: &'static [Surface],
    /// The default `help` line. Empty for a retired code.
    pub remedy: &'static str,
    /// The explanation `nros explain` prints, in Markdown. Empty for a
    /// retired code.
    pub explanation: &'static str,
    /// For a retired code, the number of the code that replaced it.
    pub retired: Option<u16>,
}

impl Entry {
    /// Whether the code is still raised.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        self.retired.is_none()
    }

    /// The code that replaced a retired one.
    #[must_use]
    pub fn successor(&self) -> Option<&'static Entry> {
        self.retired.and_then(by_number)
    }
}

/// The entry with this number, live or retired.
#[must_use]
pub fn by_number(number: u16) -> Option<&'static Entry> {
    ALL.iter().find(|e| e.number == number)
}

/// The number in a token spelled `NROS-NNNN` (exactly four digits), else
/// `None`. This is the test `nros explain` uses to tell a code from a path.
#[must_use]
pub fn parse_code(token: &str) -> Option<u16> {
    let digits = token.strip_prefix("NROS-")?;
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The entry a `NROS-NNNN` token names, live or retired.
#[must_use]
pub fn lookup(token: &str) -> Option<&'static Entry> {
    parse_code(token).and_then(by_number)
}

/// The (up to) three entries whose numbers are nearest `number`, nearest
/// first; on a tie, the lower number first.
#[must_use]
pub fn nearest(number: u16) -> [Option<&'static Entry>; 3] {
    let mut out: [Option<&'static Entry>; 3] = [None; 3];
    for e in ALL {
        let d = e.number.abs_diff(number);
        let mut slot = None;
        for (i, o) in out.iter().enumerate() {
            match o {
                None => {
                    slot = Some(i);
                    break;
                }
                Some(c) => {
                    let dc = c.number.abs_diff(number);
                    if d < dc || (d == dc && e.number < c.number) {
                        slot = Some(i);
                        break;
                    }
                }
            }
        }
        if let Some(i) = slot {
            for j in (i + 1..out.len()).rev() {
                out[j] = out[j - 1];
            }
            out[i] = Some(e);
        }
    }
    out
}

impl Diag {
    /// The code's number.
    #[must_use]
    pub const fn number(self) -> u16 {
        self as u16
    }

    /// The code's row of [`ALL`].
    #[must_use]
    pub fn entry(self) -> &'static Entry {
        by_number(self.number()).expect("every Diag variant has a row in ALL")
    }

    /// The code as printed, `NROS-NNNN`.
    #[must_use]
    pub fn code(self) -> &'static str {
        self.entry().code
    }

    /// A refusal of this diagnostic, with `detail` as the site's own words.
    /// Add a more specific remedy with [`Refusal::help`].
    #[must_use]
    pub fn refuse(self, detail: &dyn fmt::Display) -> Refusal<'_> {
        Refusal {
            diag: self,
            detail,
            help: None,
            note: None,
            verbose: false,
        }
    }

    /// Stop a build script with this diagnostic: the rendered refusal, then a
    /// panic. For a more specific remedy or a verbose note, build the
    /// [`Refusal`] with [`Diag::refuse`] and call [`Refusal::panic`].
    #[track_caller]
    pub fn panic(self, detail: fmt::Arguments<'_>) -> ! {
        self.refuse(&detail).panic()
    }
}

/// A refusal, ready to render: the site's detail, the entry's remedy (or the
/// site's own), and the line that explains it.
#[derive(Clone, Copy)]
pub struct Refusal<'a> {
    diag: Diag,
    detail: &'a dyn fmt::Display,
    help: Option<&'a dyn fmt::Display>,
    note: Option<&'a dyn fmt::Display>,
    verbose: bool,
}

impl<'a> Refusal<'a> {
    /// Replace the entry's remedy with a more specific one.
    #[must_use]
    pub fn help(mut self, help: &'a dyn fmt::Display) -> Self {
        self.help = Some(help);
        self
    }

    /// A contributor's note, printed only under verbose output. Internal ids
    /// may go here, and nowhere else in the refusal.
    #[must_use]
    pub fn note(mut self, note: &'a dyn fmt::Display) -> Self {
        self.note = Some(note);
        self
    }

    /// Whether the note prints.
    #[must_use]
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// The diagnostic this refusal raises.
    #[must_use]
    pub fn diag(&self) -> Diag {
        self.diag
    }

    /// Panic with the rendered refusal: the build-script constructor. With the
    /// `std` feature, `NROS_VERBOSE=1` turns the note on.
    #[track_caller]
    pub fn panic(self) -> ! {
        #[cfg(feature = "std")]
        let this = if std::env::var_os("NROS_VERBOSE").is_some_and(|v| v == "1") {
            self.verbose(true)
        } else {
            self
        };
        #[cfg(not(feature = "std"))]
        let this = self;
        panic!("\n{this}")
    }
}

impl fmt::Display for Refusal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let e = self.diag.entry();
        write!(f, "error[{}]: {}", e.code, self.detail)?;
        match self.help {
            Some(h) => write!(f, "\n  help: {h}")?,
            None => write!(f, "\n  help: {}", e.remedy)?,
        }
        if self.verbose
            && let Some(n) = self.note
        {
            write!(f, "\n  note: {n}")?;
        }
        write!(f, "\n  explain: nros explain {}", e.code)
    }
}

impl fmt::Debug for Refusal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests;
