//! Is this an automated session? — RFC-0097 D11.
//!
//! > phase-440 W7's pin-on-first-build (RFC-0095 D9) is right interactively and
//! > wrong in CI: silently pinning to whatever is latest produces a green build
//! > against an unrecorded toolchain, which is the reproducibility bug the pin
//! > exists to prevent.
//!
//! Two consumers, one question, so it is answered in one place:
//!
//! * [`crate::pin::pin_on_first_build`] must not WRITE a pin from CI — a pin is
//!   a source edit, and the `Cargo.lock` rule (issues 0359/0378) is that a
//!   lockfile moves when a developer means it;
//! * [`crate::launch::resolve`] must not CHOOSE a default toolchain from CI —
//!   the store's contents on a runner are incidental, so "newest installed" is
//!   a toolchain nobody chose. Same defect one step earlier: the pin would at
//!   least have been recorded.
//!
//! ## Why environment variables and not a tty probe
//!
//! `isatty` answers "is a human watching", which is nearly the right question
//! and fails on both sides here: a `nros build` inside `$(…)` on a developer's
//! machine has no tty and SHOULD pin, and a CI runner with a pty allocated
//! (`docker run -t`, `ssh -t`, most self-hosted setups) has one and MUST NOT.
//! `CI` and `GITHUB_ACTIONS` are the conventional declarations and every
//! provider sets at least the first; a declaration beats an inference.
//!
//! ## The escape hatch
//!
//! [`ALLOW_ENV`] exists because "I really do mean it" is a legitimate thing to
//! say — a container that IS the developer's machine, a release job that
//! deliberately materialises a pin. Named in every refusal, in the same family
//! as `NROS_ALLOW_SUBMODULE_REWIND` and `NROS_SKIP_STALE_CHECK`: the escape is
//! a sentence the user has to write.

/// The conventional declarations, in the order they are reported.
///
/// `CI` alone would do for every provider we have met; `GITHUB_ACTIONS` is
/// listed too so the diagnostic can name the variable that actually decided,
/// which is the first thing someone debugging an unexpected refusal asks.
pub const CI_ENV_VARS: [&str; 2] = ["CI", "GITHUB_ACTIONS"];

/// Say it anyway. Set to any non-empty value.
pub const ALLOW_ENV: &str = "NROS_ALLOW_PIN_WRITE_IN_CI";

/// Whether this process may perform a source edit (write a pin) or choose a
/// toolchain nobody named.
///
/// A value, not a function call, so every consumer's decision is a pure
/// function of its arguments and its tests need no `set_var` — which is a
/// process-global that leaks between the parallel test processes nextest runs
/// (the hazard issue 1101 documents, one crate over).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Session {
    /// `Some(var)` when an environment variable declares this an automated
    /// session AND the escape hatch is not set. The variable is carried, not
    /// just a bool, because the refusal names it.
    pub ci: Option<&'static str>,
}

impl Session {
    /// A developer at a keyboard. The default in every test that is not ABOUT
    /// the CI rule.
    #[must_use]
    pub const fn interactive() -> Self {
        Self { ci: None }
    }

    /// An automated session that declared itself through `var`.
    #[must_use]
    pub const fn automated(var: &'static str) -> Self {
        Self { ci: Some(var) }
    }

    /// Read the environment. The only place in the tree that does.
    #[must_use]
    pub fn detect() -> Self {
        Self::detect_with(|k| std::env::var_os(k))
    }

    /// [`Self::detect`] with the lookup passed IN.
    ///
    /// Not a testing convenience bolted on: `set_var` is `unsafe` in edition
    /// 2024 and is a process-global besides, so a test that set `CI` would
    /// decide the answer for every other test sharing the process. The ORDER of
    /// the two rules below is the thing worth pinning — the escape hatch has to
    /// beat the declaration, not merely appear beside it — and with the lookup
    /// as an argument that is a pure function a test can ask about directly.
    #[must_use]
    pub fn detect_with(lookup: impl Fn(&str) -> Option<std::ffi::OsString>) -> Self {
        let declared = |k: &str| lookup(k).is_some_and(|v| !v.is_empty());
        if declared(ALLOW_ENV) {
            return Self::interactive();
        }
        for var in CI_ENV_VARS {
            // A variable that is present but EMPTY is not a declaration.
            // `CI=` appears in `env -i CI= …` and in shells that export empty
            // defaults, and treating it as "this is CI" would refuse builds on
            // machines that never opted in.
            if declared(var) {
                return Self::automated(var);
            }
        }
        Self::interactive()
    }

    /// The variable that declared this automated, if any.
    #[must_use]
    pub const fn ci_var(&self) -> Option<&'static str> {
        self.ci
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<std::ffi::OsString> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |k| {
            owned
                .iter()
                .find(|(name, _)| name == k)
                .map(|(_, v)| std::ffi::OsString::from(v))
        }
    }

    /// Each conventional variable declares CI on its own, and the refusal is
    /// able to name WHICH one did.
    #[test]
    fn either_conventional_variable_declares_an_automated_session() {
        assert_eq!(Session::detect_with(env(&[])).ci_var(), None);
        assert_eq!(
            Session::detect_with(env(&[("CI", "true")])).ci_var(),
            Some("CI")
        );
        assert_eq!(
            Session::detect_with(env(&[("GITHUB_ACTIONS", "true")])).ci_var(),
            Some("GITHUB_ACTIONS")
        );
    }

    /// The escape hatch is not "unset CI" — it is a separate assertion, and it
    /// must WIN over a declaration rather than merely sitting beside it. Order
    /// the other way round and the hatch would be unreachable on every runner,
    /// which is the only place anyone would type it.
    #[test]
    fn the_escape_hatch_overrides_a_declaration() {
        assert_eq!(
            Session::detect_with(env(&[("CI", "true"), (ALLOW_ENV, "1")])).ci_var(),
            None
        );
    }

    /// `CI=` is not a declaration. Shells and `env -i CI=` produce it, and
    /// refusing on it would refuse on machines that never opted in.
    #[test]
    fn an_empty_value_is_not_a_declaration() {
        assert_eq!(Session::detect_with(env(&[("CI", "")])).ci_var(), None);
        // …and an empty escape hatch does not disarm a real declaration.
        assert_eq!(
            Session::detect_with(env(&[("CI", "true"), (ALLOW_ENV, "")])).ci_var(),
            Some("CI")
        );
    }
}
