//! The pre-build compatibility check (RFC-0135 D8, phase-577 W6).
//!
//! Before `nros build` generates or compiles anything, it compares the image
//! with its board and reports EVERY mismatch at once, so switching an image to
//! a board that cannot take it prints the whole list in one run:
//!
//! ```text
//! error[NROS-0015]: image `talker` cannot build for board `mps2-an385-baremetal` (2 problems)
//!   board: packages/boards/nros-board-mps2-an385/nros-board.toml
//!
//! error[NROS-0017]: rmw `cyclonedds` needs threads; board `mps2-an385-baremetal` states threads = false
//!   help: choose an RMW this board can run (`nros rmw list`), or a board with threads
//!   explain: nros explain NROS-0017
//!
//! error[NROS-0019]: arm-none-eabi-gcc is not installed
//!   help: run `nros setup mps2-an385-baremetal`
//!   explain: nros explain NROS-0019
//! ```
//!
//! Each existing check stays the authority for its own fact: this module asks
//! them and COLLECTS their verdicts, where each used to stop the build at the
//! first. What it adds are the two comparisons whose facts RFC-0135 D8 added
//! to the data: an RMW's `[rmw.needs]` against the board's
//! `[board.capabilities]`, and a board package's `[board.sdk] root_env`. The
//! link comparison asks the board's `[board.net] links` (RFC-0109 D3), which
//! nothing compared an image against before.
//!
//! A reason that is already a coded refusal keeps its code (the framework
//! check raises NROS-0014); a check that answers with text gets the code of
//! its D8 row.

use std::{fmt, path::Path};

use crate::{
    builder::preflight::Missing,
    diag::{Diag, DiagExt, Refusal},
    orchestration::board_descriptor::BoardCapabilities,
};

/// Every reason one image cannot build for its board.
#[derive(Debug, Clone)]
pub struct Incompatible {
    image: String,
    board: String,
    origin: Option<String>,
    reasons: Vec<Refusal>,
    missing: Vec<Missing>,
}

impl Incompatible {
    /// No reasons yet, for `image` on `board`, whose descriptor is at
    /// `origin` (named, so a user who edited one copy of a board is not
    /// debugging another).
    #[must_use]
    pub fn new(image: &str, board: &str, origin: Option<&Path>) -> Self {
        Self {
            image: image.to_string(),
            board: board.to_string(),
            origin: origin.map(|p| p.display().to_string()),
            reasons: Vec::new(),
            missing: Vec::new(),
        }
    }

    /// Add a reason.
    pub fn push(&mut self, reason: Refusal) {
        self.reasons.push(reason);
    }

    /// Add every reason in `reasons`.
    pub fn extend(&mut self, reasons: impl IntoIterator<Item = Refusal>) {
        self.reasons.extend(reasons);
    }

    /// Add the refusal `result` carries, if any: a coded refusal as it is,
    /// any other error as `row`'s diagnostic with the error's text. Returns
    /// the success value, so a later check can use it.
    pub fn collect<T>(&mut self, row: Diag, result: eyre::Result<T>) -> Option<T> {
        match result {
            Ok(v) => Some(v),
            Err(e) => {
                self.push(reason_of(row, e));
                None
            }
        }
    }

    /// Add the toolchain preflight's verdicts (RFC-0065 D2).
    pub fn missing(&mut self, missing: impl IntoIterator<Item = Missing>) {
        self.missing.extend(missing);
    }

    /// The reasons about the image and its board, in order (the toolchain's
    /// are kept apart: see [`Self::into_result`]).
    #[must_use]
    pub fn reasons(&self) -> &[Refusal] {
        &self.reasons
    }

    /// `Ok` when nothing was found.
    ///
    /// # Errors
    ///
    /// - only tools are missing: the preflight's own
    ///   [`PreflightRefusal`](crate::builder::preflight::PreflightRefusal),
    ///   unchanged. That is a refusal of THIS HOST, which `nros configure`
    ///   tells apart from a refusal of the image (it skips the image and
    ///   records why), so it must stay that type;
    /// - anything about the image or its board: every reason, the missing
    ///   tools among them, as one report.
    pub fn into_result(mut self) -> eyre::Result<()> {
        if self.reasons.is_empty() {
            if self.missing.is_empty() {
                return Ok(());
            }
            return Err(crate::builder::preflight::PreflightRefusal(self.missing).into());
        }
        let missing = std::mem::take(&mut self.missing);
        self.reasons.extend(tools(missing));
        Err(eyre::Report::new(self))
    }
}

impl fmt::Display for Incompatible {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.reasons.len();
        write!(
            f,
            "error[{}]: image `{}` cannot build for board `{}` ({n} problem{})",
            Diag::ImageBoardIncompatible.code(),
            self.image,
            self.board,
            if n == 1 { "" } else { "s" },
        )?;
        if let Some(o) = &self.origin {
            write!(f, "\n  board: {o}")?;
        }
        for r in &self.reasons {
            write!(f, "\n\n{r}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Incompatible {}

/// `e` as a reason: the coded refusal it is (or wraps), else `row`'s
/// diagnostic with its text.
#[must_use]
pub fn reason_of(row: Diag, e: eyre::Report) -> Refusal {
    match e.downcast::<Refusal>() {
        Ok(r) => r,
        Err(e) => row.refusal(e),
    }
}

/// The capabilities `rmw` needs (`[rmw.needs]`) that `caps` does not state.
#[must_use]
pub fn rmw_needs(
    rmw: &str,
    needs: &[String],
    board: &str,
    caps: BoardCapabilities,
) -> Vec<Refusal> {
    needs
        .iter()
        .filter(|n| !has(caps, n))
        .map(|n| {
            Diag::RmwNeedsCapability
                .refusal(format_args!(
                    "rmw `{rmw}` needs {n}; board `{board}` states {n} = false"
                ))
                .help(format_args!(
                    "choose an RMW this board can run (`nros rmw list`), or a board with {n}"
                ))
        })
        .collect()
}

/// Whether `caps` states the capability named `name` (the
/// `[board.capabilities]` vocabulary).
fn has(caps: BoardCapabilities, name: &str) -> bool {
    match name {
        "heap" => caps.heap,
        "atomics" => caps.atomics,
        "threads" => caps.threads,
        // The parser admits no other name (`[rmw.needs]` is the board
        // vocabulary), so this arm is a descriptor from a newer vocabulary:
        // a need this board cannot even state.
        _ => false,
    }
}

/// The image's link (its locator's scheme, RFC-0109 D3) when the board lists
/// its links and that one is not among them. A board that lists none states
/// nothing, and is not compared; nor is a `custom` transport, which the image
/// brings itself over whatever the board has.
#[must_use]
pub fn link(image: &str, link: Option<&str>, board: &str, links: &[String]) -> Option<Refusal> {
    let link = link.filter(|l| *l != "custom")?;
    if links.is_empty() || links.iter().any(|l| l == link) {
        return None;
    }
    Some(
        Diag::LinkNotOnBoard
            .refusal(format_args!(
                "image `{image}` reaches its peers over {link}; board `{board}` has links [{}]",
                links.join(", ")
            ))
            .help(format_args!(
                "set the image's `locator` to one over a link the board has, or choose a board \
                 with {link}"
            )),
    )
}

/// The board package's vendor SDK root (`[board.sdk] root_env`) when it is
/// unset, or names no directory. `value` is the variable's value, read by
/// the caller (so a test passes one).
#[must_use]
pub fn sdk_root(board: &str, var: Option<&str>, value: Option<&str>) -> Option<Refusal> {
    let var = var?;
    let why = match value.filter(|v| !v.is_empty()) {
        None => "is not set".to_string(),
        Some(v) if Path::new(v).is_dir() => return None,
        Some(v) => format!("is `{v}`, which is not a directory"),
    };
    Some(
        Diag::BoardSdkRootUnset
            .refusal(format_args!(
                "board `{board}` builds against a vendor SDK, and its root {var} {why}"
            ))
            .help(format_args!("export {var}=<path to the vendor SDK>")),
    )
}

/// The toolchain preflight's verdicts (RFC-0065 D2), one reason each.
#[must_use]
pub fn tools(missing: Vec<Missing>) -> Vec<Refusal> {
    missing
        .into_iter()
        .map(|m| {
            Diag::BoardToolMissing
                .refusal(&m.what)
                .help(format_args!("run `{}`", m.remedy))
        })
        .collect()
}

#[cfg(test)]
mod tests;
