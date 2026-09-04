//! Cross-format bridging (RFC-0088 D3, phase-421 W3).
//!
//! `Executor::open_multi` is the one place where RFC-0088's compile-time answer
//! stops existing: one image, two backends, and — once a backend that is not
//! CDR is linked — two serialization formats. A `PubSubBridge` forwards backend
//! bytes **untouched**, so it is only correct when both sides agree on what
//! those bytes mean. Until phase-421 W3 that agreement was a doc comment
//! ("both sides must use ROS-CDR … Cross-encoding bridges would need an
//! explicit translator and are out of scope"); this module turns it into a
//! return value.
//!
//! Two things live here:
//!
//! * [`BridgeError`] — what [`PubSubBridge::new`](crate::PubSubBridge::new)
//!   returns when the two sides disagree. The comparison it reports is **one
//!   byte, once, at construction**. `pump` never compares formats.
//! * [`SerializationFormatConverter`] — the deliberate cross-format case,
//!   modelled on rosbag2's `SerializationFormatConverter`, which is the one
//!   real serializer-plugin interface in the ROS 2 stack and lives, like this
//!   one, where data leaves the live system rather than where it crosses the
//!   wire.

use nros_serdes::format::SerializationFormatId;

/// Why a [`PubSubBridge`](crate::PubSubBridge) could not be constructed.
///
/// Every variant is a *construction-time* verdict. There is deliberately no
/// runtime variant: a bridge that exists is a bridge whose two sides have
/// already been proven to agree, so the forwarding path stays a memcpy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeError {
    /// The subscription's format and the publisher's format differ, and no
    /// converter was supplied. Use
    /// [`PubSubBridge::with_converter`](crate::PubSubBridge::with_converter)
    /// to opt in to a translation.
    FormatMismatch {
        /// Format of the bytes the source subscription yields.
        ingress: SerializationFormatId,
        /// Format the destination publisher expects.
        egress: SerializationFormatId,
    },
    /// A converter was supplied, but its declared endpoints do not match the
    /// bridge's. Reported with all four discriminants so the message says which
    /// end is wrong rather than only that something is.
    ConverterMismatch {
        /// [`SerializationFormatConverter::from`] of the supplied converter.
        conv_from: SerializationFormatId,
        /// [`SerializationFormatConverter::to`] of the supplied converter.
        conv_to: SerializationFormatId,
        /// Format of the bytes the source subscription yields.
        ingress: SerializationFormatId,
        /// Format the destination publisher expects.
        egress: SerializationFormatId,
    },
    /// A converter was supplied to a bridge whose `CONV_BUF` const generic is
    /// `0`. Converting needs somewhere to put the output, and this crate is
    /// `no_std` with no allocator on the forwarding path, so the buffer is a
    /// const-generic array the caller sizes:
    /// `PubSubBridge::<RX, TX, 1024>::with_converter(…)`.
    ///
    /// The default `CONV_BUF = 0` keeps a non-converting bridge exactly the
    /// size it was before this feature existed.
    NoConversionBuffer,
}

impl core::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BridgeError::FormatMismatch { ingress, egress } => write!(
                f,
                "bridge format mismatch: subscription speaks {:?} ({}), publisher expects {:?} ({}) \
                 — pass a SerializationFormatConverter to PubSubBridge::with_converter to \
                 translate deliberately",
                ingress,
                ingress.as_str(),
                egress,
                egress.as_str(),
            ),
            BridgeError::ConverterMismatch {
                conv_from,
                conv_to,
                ingress,
                egress,
            } => write!(
                f,
                "bridge converter mismatch: converter translates {} -> {}, but the bridge carries \
                 {} -> {}",
                conv_from.as_str(),
                conv_to.as_str(),
                ingress.as_str(),
                egress.as_str(),
            ),
            BridgeError::NoConversionBuffer => f.write_str(
                "bridge converter supplied to a bridge with CONV_BUF = 0: size the conversion \
                 buffer on the type, e.g. PubSubBridge::<RX, TX, 1024>::with_converter(…)",
            ),
        }
    }
}

impl core::error::Error for BridgeError {}

/// Why a [`SerializationFormatConverter::convert`] call could not produce the
/// output bytes.
///
/// Deliberately small and `Copy`: it crosses the forwarding path, which must
/// not allocate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertError {
    /// The caller's `output` slice cannot hold the converted payload.
    /// `needed` is the converter's best estimate of the required length; a
    /// converter that cannot compute one reports `0`.
    OutputTooSmall {
        /// Bytes the converter would need, or `0` when it cannot say.
        needed: usize,
    },
    /// The input bytes are not a valid payload in the source format.
    MalformedInput,
    /// The payload is well-formed but has no representation in the target
    /// format (an unbounded sequence into a fixed-size struct, say).
    Unsupported,
}

impl core::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConvertError::OutputTooSmall { needed: 0 } => {
                f.write_str("conversion output buffer too small")
            }
            ConvertError::OutputTooSmall { needed } => {
                write!(f, "conversion output buffer too small: need {needed} bytes")
            }
            ConvertError::MalformedInput => f.write_str("input is not valid in the source format"),
            ConvertError::Unsupported => {
                f.write_str("payload has no representation in the target format")
            }
        }
    }
}

impl core::error::Error for ConvertError {}

/// Translate a payload from one serialization format into another.
///
/// # Shape, and why
///
/// rosbag2's `SerializationFormatConverter` is the model: it declares the two
/// formats it sits between and converts one message at a time. Three
/// adjustments make it fit a `no_std` bridge:
///
/// * **The output buffer belongs to the caller.** rosbag2 hands back an owned
///   `SerializedBagMessage`; this crate has no allocator on the forwarding
///   path (`nros-bridge` is `#![no_std]` unless the consumer opts into `alloc`
///   for the C ABI), and a bridge that allocated per sample would be unusable
///   on the RTOS targets it exists for. So `convert` writes into `output` and
///   returns the number of bytes written.
/// * **`from`/`to` are queried, not asserted.** The bridge validates them once
///   at construction against the two sides' [`format`](nros_node::executor::RawSubscription::format)
///   accessors, so a wrong converter is a `Result` at wiring time rather than
///   garbage on the wire.
/// * **`&'static dyn`, not a generic.** A bridge image builds its bridges from
///   a config file, so the converter cannot be a type parameter resolved by the
///   caller's source; and it must outlive the bridge, which lives forever.
///
/// # Contract
///
/// * `convert` must not allocate and must not block.
/// * `output.len()` is whatever the bridge's `CONV_BUF` was sized to; a
///   converter that needs more returns [`ConvertError::OutputTooSmall`] rather
///   than truncating.
/// * The returned length must be `<= output.len()`. The bridge trusts it as a
///   slice bound after clamping.
///
/// # Example
///
/// ```
/// use nros_bridge::{ConvertError, SerializationFormatConverter};
/// use nros_serdes::format::SerializationFormatId;
///
/// /// A stand-in that copies bytes through unchanged.
/// struct Passthrough;
///
/// impl SerializationFormatConverter for Passthrough {
///     fn from(&self) -> SerializationFormatId {
///         SerializationFormatId::Cdr
///     }
///     fn to(&self) -> SerializationFormatId {
///         SerializationFormatId::Uorb
///     }
///     fn convert(&self, input: &[u8], output: &mut [u8]) -> Result<usize, ConvertError> {
///         if output.len() < input.len() {
///             return Err(ConvertError::OutputTooSmall {
///                 needed: input.len(),
///             });
///         }
///         output[..input.len()].copy_from_slice(input);
///         Ok(input.len())
///     }
/// }
/// ```
pub trait SerializationFormatConverter {
    /// The format this converter reads.
    fn from(&self) -> SerializationFormatId;

    /// The format this converter writes.
    fn to(&self) -> SerializationFormatId;

    /// Convert `input` (in [`from`](Self::from)) into `output` (in
    /// [`to`](Self::to)), returning the number of bytes written.
    fn convert(&self, input: &[u8], output: &mut [u8]) -> Result<usize, ConvertError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Noop;

    impl SerializationFormatConverter for Noop {
        fn from(&self) -> SerializationFormatId {
            SerializationFormatId::Cdr
        }
        fn to(&self) -> SerializationFormatId {
            SerializationFormatId::Uorb
        }
        fn convert(&self, input: &[u8], output: &mut [u8]) -> Result<usize, ConvertError> {
            if output.len() < input.len() {
                return Err(ConvertError::OutputTooSmall {
                    needed: input.len(),
                });
            }
            output[..input.len()].copy_from_slice(input);
            Ok(input.len())
        }
    }

    /// The error must NAME both formats — that is the whole point of turning
    /// the doc comment into a return value.
    #[test]
    fn format_mismatch_names_both_formats() {
        let e = BridgeError::FormatMismatch {
            ingress: SerializationFormatId::Cdr,
            egress: SerializationFormatId::Uorb,
        };
        let buf = render(&e);
        let s = buf.as_str();
        assert!(s.contains("cdr"), "message must name the ingress: {s}");
        assert!(s.contains("uorb"), "message must name the egress: {s}");
    }

    #[test]
    fn converter_mismatch_names_all_four() {
        let e = BridgeError::ConverterMismatch {
            conv_from: SerializationFormatId::Uorb,
            conv_to: SerializationFormatId::Cdr,
            ingress: SerializationFormatId::Cdr,
            egress: SerializationFormatId::Uorb,
        };
        let buf = render(&e);
        let s = buf.as_str();
        assert!(s.contains("uorb -> cdr"), "converter direction: {s}");
        assert!(s.contains("cdr -> uorb"), "bridge direction: {s}");
    }

    #[test]
    fn converter_declares_its_endpoints() {
        let c = Noop;
        assert_eq!(c.from(), SerializationFormatId::Cdr);
        assert_eq!(c.to(), SerializationFormatId::Uorb);
    }

    #[test]
    fn converter_refuses_a_short_output() {
        let c = Noop;
        let mut out = [0u8; 2];
        assert_eq!(
            c.convert(&[1, 2, 3, 4], &mut out),
            Err(ConvertError::OutputTooSmall { needed: 4 })
        );
        let mut out = [0u8; 8];
        assert_eq!(c.convert(&[1, 2, 3, 4], &mut out), Ok(4));
        assert_eq!(&out[..4], &[1, 2, 3, 4]);
    }

    /// Fixed-capacity `core::fmt::Write` sink — the crate is `no_std` and
    /// these tests must not need `alloc` to run.
    struct Buf {
        bytes: [u8; 512],
        len: usize,
    }

    impl Buf {
        fn as_str(&self) -> &str {
            core::str::from_utf8(&self.bytes[..self.len]).expect("utf-8")
        }
    }

    impl core::fmt::Write for Buf {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let b = s.as_bytes();
            if self.len + b.len() > self.bytes.len() {
                // Truncating would make the assertions below vacuous.
                return Err(core::fmt::Error);
            }
            self.bytes[self.len..self.len + b.len()].copy_from_slice(b);
            self.len += b.len();
            Ok(())
        }
    }

    fn render<T: core::fmt::Display>(v: &T) -> Buf {
        use core::fmt::Write as _;
        let mut b = Buf {
            bytes: [0u8; 512],
            len: 0,
        };
        write!(&mut b, "{v}").expect("BridgeError render must fit in 512 bytes");
        b
    }
}
