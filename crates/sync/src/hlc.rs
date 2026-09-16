//! Hybrid logical clock (design report §16.3).
//!
//! Wire format: `physicalMs-counter-deviceId` with `physicalMs` as 16 hex
//! digits and `counter` as 8 hex digits, e.g.
//! `0000018e7d6ab5c0-00000007-01932b39-...`.
//!
//! Lexicographic order of the clock string equals causal order, with the device
//! identifier only breaking exact ties. The clock stays monotonic even when the
//! wall clock jumps backwards, which is what keeps progress and tombstones sane
//! on a device with a wrong clock.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Failure modes for clock operations and parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HlcError {
    /// The clock string did not have three dash-separated components.
    #[error("hybrid logical clock must have the form `<physical>-<counter>-<device>`, got {0:?}")]
    Malformed(String),
    /// A component was not valid hex of the expected width.
    #[error("invalid {field} component {value:?} in hybrid logical clock")]
    InvalidComponent {
        /// `physical` or `counter`.
        field: &'static str,
        /// The offending text.
        value: String,
    },
    /// The device identifier was empty or contained whitespace.
    #[error("invalid device id: {0}")]
    InvalidDevice(String),
    /// The logical counter overflowed; the clock cannot stay monotonic.
    #[error("logical counter overflowed, refusing to produce a non-monotonic clock")]
    CounterOverflow,
}

/// Identifier of the device that wrote a value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(String);

impl DeviceId {
    /// Validate and wrap a device identifier.
    pub fn parse(value: impl Into<String>) -> Result<Self, HlcError> {
        let value = value.into();
        if value.is_empty()
            || value.chars().count() > 64
            || value.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(HlcError::InvalidDevice(value));
        }
        Ok(DeviceId(value))
    }

    /// Borrow the identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for DeviceId {
    type Err = HlcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        DeviceId::parse(s)
    }
}

impl Serialize for DeviceId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DeviceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        DeviceId::parse(raw).map_err(D::Error::custom)
    }
}

/// A hybrid logical clock value.
///
/// Ordering is `(physical_ms, counter, device_id)`: a total, deterministic order
/// across devices, which is what convergent merging requires.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hlc {
    physical_ms: u64,
    counter: u32,
    device: DeviceId,
}

impl Hlc {
    /// Build a clock value from its components.
    pub fn new(physical_ms: u64, counter: u32, device: DeviceId) -> Self {
        Self {
            physical_ms,
            counter,
            device,
        }
    }

    /// Wall-clock component in milliseconds.
    pub fn physical_ms(&self) -> u64 {
        self.physical_ms
    }

    /// Logical counter component.
    pub fn counter(&self) -> u32 {
        self.counter
    }

    /// Device that produced the value.
    pub fn device(&self) -> &DeviceId {
        &self.device
    }
}

impl fmt::Display for Hlc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:016x}-{:08x}-{}",
            self.physical_ms, self.counter, self.device
        )
    }
}

impl Ord for Hlc {
    fn cmp(&self, other: &Self) -> Ordering {
        self.physical_ms
            .cmp(&other.physical_ms)
            .then_with(|| self.counter.cmp(&other.counter))
            .then_with(|| self.device.cmp(&other.device))
    }
}

impl PartialOrd for Hlc {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl FromStr for Hlc {
    type Err = HlcError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.splitn(3, '-');
        let physical = parts
            .next()
            .ok_or_else(|| HlcError::Malformed(s.to_string()))?;
        let counter = parts
            .next()
            .ok_or_else(|| HlcError::Malformed(s.to_string()))?;
        let device = parts
            .next()
            .ok_or_else(|| HlcError::Malformed(s.to_string()))?;

        if physical.len() != 16 || counter.len() != 8 {
            return Err(HlcError::Malformed(s.to_string()));
        }
        let physical_ms =
            u64::from_str_radix(physical, 16).map_err(|_| HlcError::InvalidComponent {
                field: "physical",
                value: physical.to_string(),
            })?;
        let counter = u32::from_str_radix(counter, 16).map_err(|_| HlcError::InvalidComponent {
            field: "counter",
            value: counter.to_string(),
        })?;
        let device = DeviceId::parse(device)?;
        Ok(Hlc {
            physical_ms,
            counter,
            device,
        })
    }
}

impl From<Hlc> for String {
    fn from(value: Hlc) -> Self {
        value.to_string()
    }
}

impl TryFrom<String> for Hlc {
    type Error = HlcError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl Serialize for Hlc {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Hlc {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(D::Error::custom)
    }
}

/// A per-device hybrid logical clock.
#[derive(Debug, Clone)]
pub struct HlcClock {
    device: DeviceId,
    last: Option<Hlc>,
}

impl HlcClock {
    /// Create a clock for one device.
    pub fn new(device: DeviceId) -> Self {
        Self {
            device,
            last: None,
        }
    }

    /// The device this clock stamps values with.
    pub fn device(&self) -> &DeviceId {
        &self.device
    }

    /// The most recent value produced or observed.
    pub fn last(&self) -> Option<&Hlc> {
        self.last.as_ref()
    }

    /// Produce the next clock value for a local event.
    ///
    /// When the wall clock has not advanced past the previous value the logical
    /// counter is incremented instead, so the clock stays strictly monotonic
    /// even if `physical_ms` goes backwards.
    pub fn tick(&mut self, physical_ms: u64) -> Result<Hlc, HlcError> {
        let next = match &self.last {
            Some(last) if physical_ms > last.physical_ms => Hlc {
                physical_ms,
                counter: 0,
                device: self.device.clone(),
            },
            Some(last) => Hlc {
                physical_ms: last.physical_ms,
                counter: counter_plus_one(last.counter)?,
                device: self.device.clone(),
            },
            None => Hlc {
                physical_ms,
                counter: 0,
                device: self.device.clone(),
            },
        };
        self.last = Some(next.clone());
        Ok(next)
    }

    /// Absorb a remote value, returning a local value strictly greater than both.
    pub fn observe(&mut self, remote: &Hlc, physical_ms: u64) -> Result<Hlc, HlcError> {
        let physical = physical_ms
            .max(remote.physical_ms)
            .max(self.last.as_ref().map_or(0, |h| h.physical_ms));
        let local_counter = self
            .last
            .as_ref()
            .filter(|h| h.physical_ms == physical)
            .map(|h| h.counter);
        let remote_counter = (remote.physical_ms == physical).then_some(remote.counter);

        let counter = match (local_counter, remote_counter) {
            (Some(local), Some(remote)) => counter_plus_one(local.max(remote))?,
            (Some(local), None) => counter_plus_one(local)?,
            (None, Some(remote)) => counter_plus_one(remote)?,
            (None, None) => 0,
        };

        let next = Hlc {
            physical_ms: physical,
            counter,
            device: self.device.clone(),
        };
        self.last = Some(next.clone());
        Ok(next)
    }
}

fn counter_plus_one(counter: u32) -> Result<u32, HlcError> {
    counter.checked_add(1).ok_or(HlcError::CounterOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str) -> DeviceId {
        DeviceId::parse(name).expect("device")
    }

    #[test]
    fn clock_string_round_trips() {
        let value = Hlc::new(
            0x18e_7d6a_b5c0,
            7,
            device("01932b39-0000-7000-8000-000000000000"),
        );
        let text = value.to_string();
        assert_eq!(
            text,
            "0000018e7d6ab5c0-00000007-01932b39-0000-7000-8000-000000000000"
        );
        assert_eq!(text.parse::<Hlc>().expect("parse"), value);
    }

    #[test]
    fn rejects_malformed_clock_strings() {
        assert!(matches!(
            "0000018e7d6ab5c0-00000007".parse::<Hlc>(),
            Err(HlcError::Malformed(_))
        ));
        assert!(matches!(
            "018e7d6ab5c0-00000007-dev".parse::<Hlc>(),
            Err(HlcError::Malformed(_))
        ));
        assert!(matches!(
            "zzzzzzzzzzzzzzzz-00000007-dev".parse::<Hlc>(),
            Err(HlcError::InvalidComponent {
                field: "physical",
                ..
            })
        ));
        assert!(matches!(
            "0000018e7d6ab5c0-zzzzzzzz-dev".parse::<Hlc>(),
            Err(HlcError::InvalidComponent {
                field: "counter",
                ..
            })
        ));
        assert!(matches!(
            "0000018e7d6ab5c0-00000007-".parse::<Hlc>(),
            Err(HlcError::InvalidDevice(_))
        ));
    }

    #[test]
    fn tick_is_monotonic_within_the_same_millisecond() {
        let mut clock = HlcClock::new(device("dev-a"));
        let first = clock.tick(1_000).expect("tick");
        let second = clock.tick(1_000).expect("tick");
        let third = clock.tick(1_001).expect("tick");
        assert!(first < second);
        assert!(second < third);
        assert_eq!(third.counter(), 0);
    }

    #[test]
    fn tick_survives_wall_clock_rollback() {
        let mut clock = HlcClock::new(device("dev-a"));
        let before = clock.tick(10_000).expect("tick");
        let after = clock.tick(1).expect("tick");
        assert!(after > before, "rollback must not rewind the clock");
        assert_eq!(after.physical_ms(), 10_000);
        assert_eq!(after.counter(), 1);
    }

    #[test]
    fn observe_absorbs_remote_time_and_stays_greater() {
        let mut clock = HlcClock::new(device("dev-a"));
        clock.tick(1_000).expect("tick");
        let remote = Hlc::new(5_000, 3, device("dev-b"));
        let observed = clock.observe(&remote, 900).expect("observe");
        assert!(observed > remote);
        assert_eq!(observed.physical_ms(), 5_000);
        assert_eq!(observed.counter(), 4);
        assert_eq!(observed.device(), &device("dev-a"));
    }

    #[test]
    fn ordering_breaks_ties_by_counter_then_device() {
        let a = Hlc::new(10, 0, device("dev-a"));
        let b = Hlc::new(10, 1, device("dev-a"));
        let c = Hlc::new(10, 1, device("dev-b"));
        assert!(a < b);
        assert!(b < c);
        assert_eq!(b.to_string().cmp(&c.to_string()), Ordering::Less);
    }

    #[test]
    fn counter_overflow_is_reported_instead_of_wrapping() {
        let mut clock = HlcClock::new(device("dev-a"));
        clock.last = Some(Hlc::new(10, u32::MAX, device("dev-a")));
        assert_eq!(clock.tick(5).unwrap_err(), HlcError::CounterOverflow);
    }

    #[test]
    fn serializes_as_a_bare_string() {
        let value = Hlc::new(1, 2, device("dev-a"));
        let json = serde_json::to_value(&value).expect("serialize");
        assert_eq!(json, serde_json::json!("0000000000000001-00000002-dev-a"));
        let back: Hlc = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, value);
        assert!(serde_json::from_value::<Hlc>(serde_json::json!("nope")).is_err());
    }
}
