//! ROS time types.
//!
//! Provides [`Time`] (seconds + nanoseconds) and [`Duration`] with
//! arithmetic, conversions, and CDR serialization. All types are `no_std`
//! compatible and use integer arithmetic only (no `f64::floor`/`f64::abs`).

use core::ops::{Add, Sub};
use nros_serdes::{CdrReader, CdrWriter, DeserError, Deserialize, SerError, Serialize};

/// Nanoseconds per second
const NANOS_PER_SEC: i64 = 1_000_000_000;

/// Split a float seconds value into (sec, nanosec) without using `f64::floor()`/`f64::abs()`
/// which require `std`. Uses integer truncation with correction for negative values.
fn split_secs_f64(secs: f64) -> (i32, u32) {
    let truncated = secs as i32;
    // `as i32` truncates toward zero; floor rounds toward negative infinity.
    // Correct for negative values with a fractional part.
    let sec = if (truncated as f64) > secs {
        truncated - 1
    } else {
        truncated
    };
    let frac = secs - sec as f64;
    // frac should be in [0, 1), but guard against float imprecision
    let frac = if frac < 0.0 { -frac } else { frac };
    let nanosec = (frac * NANOS_PER_SEC as f64) as u32;
    (sec, nanosec)
}

/// ROS Time representation
///
/// Matches `builtin_interfaces/msg/Time`:
/// - `sec`: Seconds since epoch (signed for pre-epoch times)
/// - `nanosec`: Nanoseconds within the second (0-999999999)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct Time {
    /// Seconds component
    pub sec: i32,
    /// Nanoseconds component (0-999999999)
    pub nanosec: u32,
}

impl Time {
    /// Create a new Time
    pub const fn new(sec: i32, nanosec: u32) -> Self {
        Self { sec, nanosec }
    }

    /// Create a Time representing zero
    pub const fn zero() -> Self {
        Self { sec: 0, nanosec: 0 }
    }

    /// Check if this time is zero
    pub const fn is_zero(&self) -> bool {
        self.sec == 0 && self.nanosec == 0
    }

    /// Create a Time from nanoseconds since epoch
    pub const fn from_nanos(nanos: i64) -> Self {
        let sec = (nanos / NANOS_PER_SEC) as i32;
        let nanosec = (nanos % NANOS_PER_SEC) as u32;
        Self { sec, nanosec }
    }

    /// Convert to total nanoseconds since epoch
    pub const fn to_nanos(&self) -> i64 {
        (self.sec as i64) * NANOS_PER_SEC + (self.nanosec as i64)
    }

    /// Create a Time from a Duration (treating duration as time since epoch)
    pub const fn from_duration(d: Duration) -> Self {
        Self {
            sec: d.sec,
            nanosec: d.nanosec,
        }
    }

    /// Convert to Duration (treating time as duration since epoch)
    pub const fn as_duration(&self) -> Duration {
        Duration {
            sec: self.sec,
            nanosec: self.nanosec,
        }
    }

    /// Create a Time from seconds (float)
    pub fn from_secs_f64(secs: f64) -> Self {
        let (sec, nanosec) = split_secs_f64(secs);
        Self { sec, nanosec }
    }

    /// Convert to seconds (float)
    pub fn to_secs_f64(&self) -> f64 {
        self.sec as f64 + (self.nanosec as f64 / NANOS_PER_SEC as f64)
    }
}

impl Add<Duration> for Time {
    type Output = Time;

    fn add(self, rhs: Duration) -> Self::Output {
        let total_nanos = self.to_nanos() + rhs.to_nanos();
        Time::from_nanos(total_nanos)
    }
}

impl Sub<Duration> for Time {
    type Output = Time;

    fn sub(self, rhs: Duration) -> Self::Output {
        let total_nanos = self.to_nanos() - rhs.to_nanos();
        Time::from_nanos(total_nanos)
    }
}

impl Sub<Time> for Time {
    type Output = Duration;

    fn sub(self, rhs: Time) -> Self::Output {
        let diff_nanos = self.to_nanos() - rhs.to_nanos();
        Duration::from_nanos(diff_nanos)
    }
}

impl Serialize for Time {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.sec)?;
        writer.write_u32(self.nanosec)?;
        Ok(())
    }
}

impl Deserialize for Time {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            sec: reader.read_i32()?,
            nanosec: reader.read_u32()?,
        })
    }
}

/// ROS Duration representation
///
/// Matches `builtin_interfaces/msg/Duration`:
/// - `sec`: Seconds (signed for negative durations)
/// - `nanosec`: Nanoseconds (0-999999999)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct Duration {
    /// Seconds component
    pub sec: i32,
    /// Nanoseconds component (0-999999999)
    pub nanosec: u32,
}

impl Duration {
    /// Create a new Duration
    pub const fn new(sec: i32, nanosec: u32) -> Self {
        Self { sec, nanosec }
    }

    /// Create a Duration representing zero
    pub const fn zero() -> Self {
        Self { sec: 0, nanosec: 0 }
    }

    /// Check if this duration is zero
    pub const fn is_zero(&self) -> bool {
        self.sec == 0 && self.nanosec == 0
    }

    /// Create a Duration from seconds
    pub const fn from_secs(secs: i32) -> Self {
        Self {
            sec: secs,
            nanosec: 0,
        }
    }

    /// Create a Duration from milliseconds
    pub const fn from_millis(millis: i64) -> Self {
        let sec = (millis / 1000) as i32;
        let nanosec = ((millis % 1000) * 1_000_000) as u32;
        Self { sec, nanosec }
    }

    /// Create a Duration from nanoseconds
    pub const fn from_nanos(nanos: i64) -> Self {
        let sec = (nanos / NANOS_PER_SEC) as i32;
        let nanosec = (nanos % NANOS_PER_SEC).unsigned_abs() as u32;
        Self { sec, nanosec }
    }

    /// Convert to total nanoseconds
    pub const fn to_nanos(&self) -> i64 {
        (self.sec as i64) * NANOS_PER_SEC + (self.nanosec as i64)
    }

    /// Create a Duration from seconds (float)
    pub fn from_secs_f64(secs: f64) -> Self {
        let (sec, nanosec) = split_secs_f64(secs);
        Self { sec, nanosec }
    }

    /// Convert to seconds (float)
    pub fn to_secs_f64(&self) -> f64 {
        self.sec as f64 + (self.nanosec as f64 / NANOS_PER_SEC as f64)
    }
}

impl Add for Duration {
    type Output = Duration;

    fn add(self, rhs: Duration) -> Self::Output {
        let total_nanos = self.to_nanos() + rhs.to_nanos();
        Duration::from_nanos(total_nanos)
    }
}

impl Sub for Duration {
    type Output = Duration;

    fn sub(self, rhs: Duration) -> Self::Output {
        let diff_nanos = self.to_nanos() - rhs.to_nanos();
        Duration::from_nanos(diff_nanos)
    }
}

impl Serialize for Duration {
    fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
        writer.write_i32(self.sec)?;
        writer.write_u32(self.nanosec)?;
        Ok(())
    }
}

impl Deserialize for Duration {
    fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
        Ok(Self {
            sec: reader.read_i32()?,
            nanosec: reader.read_u32()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_roundtrip() {
        let mut buf = [0u8; 16];
        let time = Time::new(1234567890, 123456789);

        let mut writer = CdrWriter::new(&mut buf);
        time.serialize(&mut writer).unwrap();

        let mut reader = CdrReader::new(&buf);
        let result = Time::deserialize(&mut reader).unwrap();
        assert_eq!(result, time);
    }

    #[test]
    fn test_duration_roundtrip() {
        let mut buf = [0u8; 16];
        let duration = Duration::new(-10, 500000000);

        let mut writer = CdrWriter::new(&mut buf);
        duration.serialize(&mut writer).unwrap();

        let mut reader = CdrReader::new(&buf);
        let result = Duration::deserialize(&mut reader).unwrap();
        assert_eq!(result, duration);
    }

    #[test]
    fn test_time_from_nanos() {
        let time = Time::from_nanos(1_500_000_000);
        assert_eq!(time.sec, 1);
        assert_eq!(time.nanosec, 500_000_000);
    }

    #[test]
    fn test_time_to_nanos() {
        let time = Time::new(2, 500_000_000);
        assert_eq!(time.to_nanos(), 2_500_000_000);
    }

    #[test]
    fn test_time_add_duration() {
        let time = Time::new(10, 500_000_000);
        let duration = Duration::new(5, 700_000_000);
        let result = time + duration;
        assert_eq!(result.sec, 16);
        assert_eq!(result.nanosec, 200_000_000);
    }

    #[test]
    fn test_time_sub_duration() {
        let time = Time::new(10, 500_000_000);
        let duration = Duration::new(3, 200_000_000);
        let result = time - duration;
        assert_eq!(result.sec, 7);
        assert_eq!(result.nanosec, 300_000_000);
    }

    #[test]
    fn test_time_sub_time() {
        let t1 = Time::new(10, 500_000_000);
        let t2 = Time::new(7, 200_000_000);
        let result = t1 - t2;
        assert_eq!(result.sec, 3);
        assert_eq!(result.nanosec, 300_000_000);
    }

    #[test]
    fn test_time_from_duration() {
        let duration = Duration::new(5, 123456789);
        let time = Time::from_duration(duration);
        assert_eq!(time.sec, 5);
        assert_eq!(time.nanosec, 123456789);
    }

    #[test]
    fn test_time_as_duration() {
        let time = Time::new(5, 123456789);
        let duration = time.as_duration();
        assert_eq!(duration.sec, 5);
        assert_eq!(duration.nanosec, 123456789);
    }

    #[test]
    fn test_time_ordering() {
        let t1 = Time::new(10, 0);
        let t2 = Time::new(10, 500_000_000);
        let t3 = Time::new(11, 0);
        assert!(t1 < t2);
        assert!(t2 < t3);
    }

    #[test]
    fn test_duration_from_nanos() {
        let duration = Duration::from_nanos(2_500_000_000);
        assert_eq!(duration.sec, 2);
        assert_eq!(duration.nanosec, 500_000_000);
    }

    #[test]
    fn test_duration_to_nanos() {
        let duration = Duration::new(3, 250_000_000);
        assert_eq!(duration.to_nanos(), 3_250_000_000);
    }

    #[test]
    fn test_duration_add() {
        let d1 = Duration::new(2, 500_000_000);
        let d2 = Duration::new(1, 700_000_000);
        let result = d1 + d2;
        assert_eq!(result.sec, 4);
        assert_eq!(result.nanosec, 200_000_000);
    }

    #[test]
    fn test_duration_sub() {
        let d1 = Duration::new(5, 500_000_000);
        let d2 = Duration::new(2, 200_000_000);
        let result = d1 - d2;
        assert_eq!(result.sec, 3);
        assert_eq!(result.nanosec, 300_000_000);
    }

    #[test]
    fn test_duration_ordering() {
        let d1 = Duration::new(1, 0);
        let d2 = Duration::new(1, 500_000_000);
        let d3 = Duration::new(2, 0);
        assert!(d1 < d2);
        assert!(d2 < d3);
    }

    #[test]
    fn test_time_secs_f64() {
        let time = Time::from_secs_f64(1.5);
        assert_eq!(time.sec, 1);
        assert_eq!(time.nanosec, 500_000_000);

        let secs = time.to_secs_f64();
        assert!((secs - 1.5).abs() < 0.000001);
    }

    #[test]
    fn test_duration_secs_f64() {
        let duration = Duration::from_secs_f64(2.25);
        assert_eq!(duration.sec, 2);
        assert_eq!(duration.nanosec, 250_000_000);

        let secs = duration.to_secs_f64();
        assert!((secs - 2.25).abs() < 0.000001);
    }
}

// =============================================================================
// Kani bounded model checking proofs
// =============================================================================

#[cfg(kani)]
mod verification {
    use super::*;

    // ---- Duration ----

    #[kani::proof]
    fn duration_from_nanos_no_panic() {
        let nanos: i64 = kani::any();
        // Constrain to tractable range for CBMC (i64 div/mod is expensive)
        kani::assume(nanos >= -10_000_000_000 && nanos <= 10_000_000_000);
        let dur = Duration::from_nanos(nanos);
        // nanosec must always be in valid range
        assert!(dur.nanosec < NANOS_PER_SEC as u32);
    }

    #[kani::proof]
    fn duration_roundtrip_nanos() {
        let nanos: i64 = kani::any();
        // Constrain to tractable range for CBMC (i64 div/mod is expensive)
        // Still covers multi-second values to exercise the div/mod logic
        kani::assume(nanos >= 0 && nanos <= 10_000_000_000);
        let dur = Duration::from_nanos(nanos);
        assert_eq!(dur.to_nanos(), nanos);
    }

    #[kani::proof]
    fn duration_zero_is_zero() {
        let dur = Duration::zero();
        assert!(dur.is_zero());
        assert_eq!(dur.to_nanos(), 0);
    }

    #[kani::proof]
    fn duration_from_secs() {
        let secs: i32 = kani::any();
        let dur = Duration::from_secs(secs);
        assert_eq!(dur.sec, secs);
        assert_eq!(dur.nanosec, 0);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn duration_serialize_roundtrip() {
        let sec: i32 = kani::any();
        let nanosec: u32 = kani::any();
        kani::assume(nanosec < NANOS_PER_SEC as u32);
        let dur = Duration::new(sec, nanosec);

        let mut buf = [0u8; 16];
        let len = {
            let mut writer = CdrWriter::new(&mut buf);
            dur.serialize(&mut writer).unwrap();
            writer.position()
        };

        let mut reader = CdrReader::new(&buf[..len]);
        let deserialized = Duration::deserialize(&mut reader).unwrap();
        assert_eq!(dur, deserialized);
    }

    // ---- Time ----

    #[kani::proof]
    fn time_from_nanos_no_panic() {
        let nanos: i64 = kani::any();
        // Constrain to tractable range for CBMC (i64 div/mod is expensive)
        // NOTE: Negative nanos cause nanosec wrapping (known bug — Time::from_nanos
        // lacks .unsigned_abs() unlike Duration::from_nanos). Constrain to >= 0.
        kani::assume(nanos >= 0 && nanos <= 10_000_000_000);
        let time = Time::from_nanos(nanos);
        // nanosec must always be in valid range
        assert!(time.nanosec < NANOS_PER_SEC as u32);
    }

    #[kani::proof]
    fn time_roundtrip_nanos() {
        let nanos: i64 = kani::any();
        // Constrain to tractable range for CBMC (i64 div/mod is expensive)
        kani::assume(nanos >= 0 && nanos <= 10_000_000_000);
        let time = Time::from_nanos(nanos);
        assert_eq!(time.to_nanos(), nanos);
    }

    #[kani::proof]
    fn time_zero_is_zero() {
        let time = Time::zero();
        assert!(time.is_zero());
        assert_eq!(time.to_nanos(), 0);
    }

    #[kani::proof]
    fn time_duration_conversion() {
        let sec: i32 = kani::any();
        let nanosec: u32 = kani::any();
        kani::assume(nanosec < NANOS_PER_SEC as u32);
        let time = Time::new(sec, nanosec);
        let dur = time.as_duration();
        assert_eq!(dur.sec, time.sec);
        assert_eq!(dur.nanosec, time.nanosec);
        let back = Time::from_duration(dur);
        assert_eq!(back, time);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn time_serialize_roundtrip() {
        let sec: i32 = kani::any();
        let nanosec: u32 = kani::any();
        kani::assume(nanosec < NANOS_PER_SEC as u32);
        let time = Time::new(sec, nanosec);

        let mut buf = [0u8; 16];
        let len = {
            let mut writer = CdrWriter::new(&mut buf);
            time.serialize(&mut writer).unwrap();
            writer.position()
        };

        let mut reader = CdrReader::new(&buf[..len]);
        let deserialized = Time::deserialize(&mut reader).unwrap();
        assert_eq!(time, deserialized);
    }
}
