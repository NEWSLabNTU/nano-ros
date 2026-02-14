//! Executor trigger conditions
//!
//! Trigger conditions control *when* an executor processes callbacks during
//! `spin_once()`. Instead of always processing all ready handles, triggers
//! let you specify conditions like "only process when ALL handles have data"
//! or "only process when a specific handle has data".
//!
//! # Built-in Conditions
//!
//! - [`TriggerCondition::Any`] — Process when *any* handle has data (default)
//! - [`TriggerCondition::All`] — Process only when *all* handles have data
//! - [`TriggerCondition::Always`] — Always process (unconditionally)
//! - [`TriggerCondition::One(index)`] — Process when handle at `index` has data
//!
//! # Custom Triggers
//!
//! Use a function pointer for `no_std` targets, or a boxed closure on `std`:
//!
//! ```ignore
//! // Function pointer (no_std compatible)
//! fn my_trigger(ready: &[bool]) -> bool {
//!     ready.get(0).copied().unwrap_or(false) && ready.get(1).copied().unwrap_or(false)
//! }
//! executor.set_custom_trigger(my_trigger);
//!
//! // Boxed closure (std only)
//! executor.set_trigger_fn(|ready| ready.iter().filter(|&&r| r).count() >= 2);
//! ```

/// Built-in trigger conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerCondition {
    /// Process when any handle has data ready (default behavior)
    Any,
    /// Process only when all handles have data ready
    All,
    /// Always process, regardless of data availability
    Always,
    /// Process when the handle at the given index has data ready
    One(usize),
}

/// Function pointer type for custom trigger conditions
///
/// Receives a slice of booleans indicating which handles have data ready.
/// Returns true if the executor should process callbacks.
pub type TriggerFn = fn(&[bool]) -> bool;

/// Trigger configuration for an executor
pub enum Trigger {
    /// A built-in trigger condition
    Builtin(TriggerCondition),
    /// A custom trigger function pointer (no_std compatible)
    Custom(TriggerFn),
    /// A boxed closure trigger (std only)
    #[cfg(feature = "std")]
    #[allow(clippy::type_complexity)]
    Boxed(alloc::boxed::Box<dyn Fn(&[bool]) -> bool + Send>),
}

impl Default for Trigger {
    fn default() -> Self {
        Trigger::Builtin(TriggerCondition::Any)
    }
}

impl core::fmt::Debug for Trigger {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Trigger::Builtin(cond) => f.debug_tuple("Builtin").field(cond).finish(),
            Trigger::Custom(_) => f
                .debug_tuple("Custom")
                .field(&"fn(&[bool]) -> bool")
                .finish(),
            #[cfg(feature = "std")]
            Trigger::Boxed(_) => f
                .debug_tuple("Boxed")
                .field(&"dyn Fn(&[bool]) -> bool")
                .finish(),
        }
    }
}

impl Trigger {
    /// Evaluate the trigger condition against the ready mask
    ///
    /// Returns true if the executor should process callbacks.
    pub fn evaluate(&self, ready: &[bool]) -> bool {
        match self {
            Trigger::Builtin(cond) => cond.evaluate(ready),
            Trigger::Custom(f) => f(ready),
            #[cfg(feature = "std")]
            Trigger::Boxed(f) => f(ready),
        }
    }
}

impl TriggerCondition {
    /// Evaluate this condition against the ready mask
    pub fn evaluate(&self, ready: &[bool]) -> bool {
        match self {
            TriggerCondition::Any => ready.iter().any(|&r| r),
            TriggerCondition::All => !ready.is_empty() && ready.iter().all(|&r| r),
            TriggerCondition::Always => true,
            TriggerCondition::One(index) => ready.get(*index).copied().unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_any() {
        let cond = TriggerCondition::Any;
        assert!(cond.evaluate(&[true, false, false]));
        assert!(cond.evaluate(&[false, true, false]));
        assert!(cond.evaluate(&[true, true, true]));
        assert!(!cond.evaluate(&[false, false, false]));
    }

    #[test]
    fn test_trigger_all() {
        let cond = TriggerCondition::All;
        assert!(cond.evaluate(&[true, true, true]));
        assert!(!cond.evaluate(&[true, false, true]));
        assert!(!cond.evaluate(&[false, false, false]));
    }

    #[test]
    fn test_trigger_always() {
        let cond = TriggerCondition::Always;
        assert!(cond.evaluate(&[false, false]));
        assert!(cond.evaluate(&[true, true]));
        assert!(cond.evaluate(&[]));
    }

    #[test]
    fn test_trigger_one() {
        let cond = TriggerCondition::One(1);
        assert!(cond.evaluate(&[false, true, false]));
        assert!(!cond.evaluate(&[true, false, true]));
        // Out of bounds returns false
        assert!(!cond.evaluate(&[true]));
    }

    #[test]
    fn test_trigger_custom() {
        fn at_least_two(ready: &[bool]) -> bool {
            ready.iter().filter(|&&r| r).count() >= 2
        }

        let trigger = Trigger::Custom(at_least_two);
        assert!(trigger.evaluate(&[true, true, false]));
        assert!(!trigger.evaluate(&[true, false, false]));
        assert!(trigger.evaluate(&[true, true, true]));
    }

    #[test]
    fn test_trigger_empty_mask() {
        // Edge case: no handles registered
        assert!(!TriggerCondition::Any.evaluate(&[]));
        assert!(!TriggerCondition::All.evaluate(&[]));
        assert!(TriggerCondition::Always.evaluate(&[]));
        assert!(!TriggerCondition::One(0).evaluate(&[]));
    }

    #[test]
    fn test_trigger_default() {
        let trigger = Trigger::default();
        // Default is Any
        assert!(trigger.evaluate(&[true, false]));
        assert!(!trigger.evaluate(&[false, false]));
    }

    /// Sensor fusion scenario: two subscriptions (IMU + LIDAR) must both have
    /// data before the executor should process callbacks.
    #[test]
    fn test_sensor_fusion_scenario() {
        let trigger = Trigger::Builtin(TriggerCondition::All);

        // ready_mask[0] = IMU subscription, ready_mask[1] = LIDAR subscription

        // Neither sensor has data → don't process
        assert!(!trigger.evaluate(&[false, false]));

        // Only IMU has data → don't process (waiting for LIDAR)
        assert!(!trigger.evaluate(&[true, false]));

        // Only LIDAR has data → don't process (waiting for IMU)
        assert!(!trigger.evaluate(&[false, true]));

        // Both sensors have data → process!
        assert!(trigger.evaluate(&[true, true]));
    }

    /// Sensor fusion with a custom trigger: process when at least 2 out of 3
    /// sensors have data (majority voting).
    #[test]
    fn test_sensor_fusion_majority_voting() {
        fn majority(ready: &[bool]) -> bool {
            ready.iter().filter(|&&r| r).count() >= 2
        }

        let trigger = Trigger::Custom(majority);

        // ready_mask: [IMU, LIDAR, GPS]
        assert!(!trigger.evaluate(&[false, false, false]));
        assert!(!trigger.evaluate(&[true, false, false]));
        assert!(trigger.evaluate(&[true, true, false]));
        assert!(trigger.evaluate(&[true, false, true]));
        assert!(trigger.evaluate(&[true, true, true]));
    }

    /// Trigger::One used for priority-based processing: only process
    /// when the critical sensor (e.g. emergency stop) has data.
    #[test]
    fn test_priority_sensor_trigger() {
        // Handle 0 = emergency stop, Handle 1 = navigation, Handle 2 = diagnostics
        let trigger = Trigger::Builtin(TriggerCondition::One(0));

        // Emergency stop not ready → skip
        assert!(!trigger.evaluate(&[false, true, true]));

        // Emergency stop ready → process
        assert!(trigger.evaluate(&[true, false, false]));
        assert!(trigger.evaluate(&[true, true, true]));
    }
}
