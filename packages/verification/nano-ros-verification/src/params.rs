/// ParameterValue + range proofs (Phase 31.4)
///
/// Proves correctness of parameter range containment and type tagging.
///
/// ## Trust levels
///
/// **Formally linked** (via `assume_specification` + `external_type_specification`):
/// - `IntegerRange` — transparent struct, pub fields (min: i64, max: i64, step: i64).
/// - `ParameterType` — transparent enum, 10 variants (NotSet through StringArray).
/// - `IntegerRange::contains()` — linked to `integer_range_contains_spec`.
///
/// **Ghost model** (manually audited mirror of production code):
/// - `ParameterValueGhost` — mirrors `ParameterValue` discriminant structure.
///   Array/string payloads are abstracted away (heapless types can't be imported
///   into Verus). Correctness relies on line-by-line variant correspondence with
///   `nano-ros-params/src/types.rs:52-81`.
/// - `FloatRangeGhost` — mirrors `FloatingPointRange` using int fields because
///   Verus does not support f64 reasoning. Proves the same structural containment
///   property.
use vstd::prelude::*;
use nano_ros_params::{IntegerRange, ParameterType};

verus! {

// ======================================================================
// Type Specifications
// ======================================================================

/// Register `IntegerRange` with Verus as a transparent type.
/// Pub fields: `min: i64`, `max: i64`, `step: i64`.
///
/// Source (types.rs:215-225):
/// ```ignore
/// pub struct IntegerRange {
///     pub min: i64,
///     pub max: i64,
///     pub step: i64,
/// }
/// ```
#[verifier::external_type_specification]
pub struct ExIntegerRange(IntegerRange);

/// Register `ParameterType` with Verus as a transparent type.
///
/// Source (types.rs:23-50):
/// ```ignore
/// #[repr(u8)]
/// pub enum ParameterType {
///     NotSet = 0, Bool = 1, Integer = 2, Double = 3, String = 4,
///     ByteArray = 5, BoolArray = 6, IntegerArray = 7, DoubleArray = 8, StringArray = 9,
/// }
/// ```
#[verifier::external_type_specification]
pub struct ExParameterType(ParameterType);

// ======================================================================
// Spec Functions
// ======================================================================

/// Spec: `IntegerRange::contains()` — value within [min, max].
///
/// Mirrors (types.rs:233-235):
/// ```ignore
/// pub fn contains(&self, value: i64) -> bool {
///     value >= self.min && value <= self.max
/// }
/// ```
pub open spec fn integer_range_contains_spec(range: IntegerRange, value: i64) -> bool {
    value >= range.min && value <= range.max
}

// ======================================================================
// Trusted Contracts
// ======================================================================

/// Trusted contract: `IntegerRange::contains()` matches `integer_range_contains_spec`.
///
/// A human auditor should confirm the 2-condition check matches types.rs:233-235.
pub assume_specification[ IntegerRange::contains ](self_: &IntegerRange, value: i64) -> (ret: bool)
    ensures
        ret == integer_range_contains_spec(*self_, value);

// ======================================================================
// Ghost Models
// ======================================================================

/// Ghost representation of `FloatingPointRange` using int fields.
///
/// Verus does not support f64 reasoning, so we model the range containment
/// logic abstractly. The structural property (boundary containment) is
/// identical to IntegerRange — only the numeric type differs.
///
/// Source (types.rs:192-213):
/// ```ignore
/// pub struct FloatingPointRange {
///     pub min: f64,
///     pub max: f64,
///     pub step: f64,
/// }
/// impl FloatingPointRange {
///     pub fn contains(&self, value: f64) -> bool {
///         value >= self.min && value <= self.max
///     }
/// }
/// ```
pub struct FloatRangeGhost {
    pub min: int,
    pub max: int,
    pub step: int,
}

/// Spec: `FloatRangeGhost::contains()` — abstract range containment.
pub open spec fn float_range_contains_ghost(range: FloatRangeGhost, value: int) -> bool {
    value >= range.min && value <= range.max
}

/// Ghost representation of `ParameterValue` discriminant structure.
///
/// Mirrors 10 variants from `nano-ros-params/src/types.rs:52-81`.
/// Array and string payloads are abstracted (heapless types not importable
/// into Verus). Scalar payloads (bool, i64) are preserved for roundtrip proofs.
///
/// Source (types.rs:52-81):
/// ```ignore
/// pub enum ParameterValue {
///     NotSet, Bool(bool), Integer(i64), Double(f64),
///     String(...), ByteArray(...), BoolArray(...),
///     IntegerArray(...), DoubleArray(...), StringArray(...),
/// }
/// ```
pub enum ParameterValueGhost {
    NotSet,
    Bool(bool),
    Integer(i64),
    Double,        // f64 payload abstracted (Verus has no f64 support)
    String,        // heapless::String payload abstracted
    ByteArray,     // heapless::Vec<u8> payload abstracted
    BoolArray,     // heapless::Vec<bool> payload abstracted
    IntegerArray,  // heapless::Vec<i64> payload abstracted
    DoubleArray,   // heapless::Vec<f64> payload abstracted
    StringArray,   // heapless::Vec<String> payload abstracted
}

/// Spec: `ParameterValue::param_type()` — maps variant to ParameterType tag.
///
/// Mirrors (types.rs:84-98):
/// ```ignore
/// pub fn param_type(&self) -> ParameterType {
///     match self {
///         Self::NotSet => ParameterType::NotSet,
///         Self::Bool(_) => ParameterType::Bool,
///         ...
///     }
/// }
/// ```
pub open spec fn param_type_spec(v: ParameterValueGhost) -> ParameterType {
    match v {
        ParameterValueGhost::NotSet => ParameterType::NotSet,
        ParameterValueGhost::Bool(_) => ParameterType::Bool,
        ParameterValueGhost::Integer(_) => ParameterType::Integer,
        ParameterValueGhost::Double => ParameterType::Double,
        ParameterValueGhost::String => ParameterType::String,
        ParameterValueGhost::ByteArray => ParameterType::ByteArray,
        ParameterValueGhost::BoolArray => ParameterType::BoolArray,
        ParameterValueGhost::IntegerArray => ParameterType::IntegerArray,
        ParameterValueGhost::DoubleArray => ParameterType::DoubleArray,
        ParameterValueGhost::StringArray => ParameterType::StringArray,
    }
}

// ======================================================================
// Proofs
// ======================================================================

/// **Proof: `integer_range_contains_boundary`**
///
/// An IntegerRange with `min <= max` contains both its boundary values.
/// Combined with the assume_specification, this proves that the production
/// `IntegerRange::contains()` returns true for min and max.
///
/// Real-time relevance: Parameter validation accepts boundary values.
proof fn integer_range_contains_boundary(range: IntegerRange)
    requires
        range.min <= range.max,
    ensures
        integer_range_contains_spec(range, range.min),
        integer_range_contains_spec(range, range.max),
{
}

/// **Proof: `float_range_contains_boundary`**
///
/// Structural containment property for floating-point ranges, proved on a
/// ghost model with int fields (since Verus does not support f64 reasoning).
///
/// The logic is identical to IntegerRange: `min <= value <= max` holds for
/// both boundary values. A human auditor confirms the same logic structure
/// in FloatingPointRange::contains (types.rs:210-212).
///
/// Real-time relevance: Float parameter validation accepts boundary values.
proof fn float_range_contains_boundary(range: FloatRangeGhost)
    requires
        range.min <= range.max,
    ensures
        float_range_contains_ghost(range, range.min),
        float_range_contains_ghost(range, range.max),
{
}

/// **Proof: `parameter_value_roundtrip`**
///
/// Scalar values (i64, bool) survive the ParameterValue wrapping:
/// - `Integer(v)` extracts to `v` for all i64
/// - `Bool(v)` extracts to `v` for all bool
///
/// This is trivially true by construction (pattern matching), but the formal
/// proof establishes the property for the ghost model and documents the
/// correspondence with the production type.
///
/// Real-time relevance: Parameter values stored via declare() and retrieved
/// via get() return the exact original value.
proof fn parameter_value_roundtrip(v_i64: i64, v_bool: bool)
    ensures
        // Integer roundtrip
        match ParameterValueGhost::Integer(v_i64) {
            ParameterValueGhost::Integer(x) => x == v_i64,
            _ => false,
        },
        // Bool roundtrip
        match ParameterValueGhost::Bool(v_bool) {
            ParameterValueGhost::Bool(x) => x == v_bool,
            _ => false,
        },
{
}

/// **Proof: `parameter_value_type_tag`**
///
/// Each ParameterValueGhost variant maps to the correct ParameterType
/// discriminant via `param_type_spec`. This exhaustively verifies all 10
/// variant-to-tag mappings.
///
/// Combined with the ghost model correspondence, this proves that the
/// production `ParameterValue::param_type()` returns the correct type tag.
///
/// Real-time relevance: Parameter type introspection is correct.
proof fn parameter_value_type_tag()
    ensures
        param_type_spec(ParameterValueGhost::NotSet) is NotSet,
        param_type_spec(ParameterValueGhost::Bool(true)) is Bool,
        param_type_spec(ParameterValueGhost::Bool(false)) is Bool,
        param_type_spec(ParameterValueGhost::Integer(0)) is Integer,
        param_type_spec(ParameterValueGhost::Double) is Double,
        param_type_spec(ParameterValueGhost::String) is String,
        param_type_spec(ParameterValueGhost::ByteArray) is ByteArray,
        param_type_spec(ParameterValueGhost::BoolArray) is BoolArray,
        param_type_spec(ParameterValueGhost::IntegerArray) is IntegerArray,
        param_type_spec(ParameterValueGhost::DoubleArray) is DoubleArray,
        param_type_spec(ParameterValueGhost::StringArray) is StringArray,
{
}

} // verus!
