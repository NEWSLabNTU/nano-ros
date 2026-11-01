//! Exit codes.
//!
//! A runner exits with one of three codes (RFC-0119 D6). nros's own run,
//! flash and monitor verbs use the same three plus [`PLAN_UNRESOLVED`]
//! (RFC-0118 D2), which only nros can produce: a runner is handed a plan that
//! has already resolved.

/// Success, or `--expect` matched.
pub const SUCCESS: u8 = 0;
/// The target failed, or `--expect` timed out.
pub const TARGET_FAILED: u8 = 1;
/// The runner failed: it could not launch, flash or open the console.
pub const RUNNER_FAILED: u8 = 2;
/// nros could not resolve a plan. Never a runner's exit code.
pub const PLAN_UNRESOLVED: u8 = 3;

/// A runner's exit status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerExit {
    /// [`SUCCESS`]
    Success,
    /// [`TARGET_FAILED`]
    TargetFailed,
    /// [`RUNNER_FAILED`]
    RunnerFailed,
}

impl RunnerExit {
    /// The numeric code.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            RunnerExit::Success => SUCCESS,
            RunnerExit::TargetFailed => TARGET_FAILED,
            RunnerExit::RunnerFailed => RUNNER_FAILED,
        }
    }

    /// Classify a runner process's exit code, as nros does. Anything outside
    /// the contract (a signal, a stray code) is the runner failing.
    #[must_use]
    pub fn from_code(code: Option<i32>) -> RunnerExit {
        match code {
            Some(0) => RunnerExit::Success,
            Some(1) => RunnerExit::TargetFailed,
            _ => RunnerExit::RunnerFailed,
        }
    }
}

impl From<RunnerExit> for std::process::ExitCode {
    fn from(e: RunnerExit) -> Self {
        std::process::ExitCode::from(e.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_and_strays_are_runner_failures() {
        for e in [
            RunnerExit::Success,
            RunnerExit::TargetFailed,
            RunnerExit::RunnerFailed,
        ] {
            assert_eq!(RunnerExit::from_code(Some(i32::from(e.code()))), e);
        }
        assert_eq!(RunnerExit::from_code(None), RunnerExit::RunnerFailed);
        assert_eq!(RunnerExit::from_code(Some(3)), RunnerExit::RunnerFailed);
        assert_eq!(RunnerExit::from_code(Some(-1)), RunnerExit::RunnerFailed);
    }
}
