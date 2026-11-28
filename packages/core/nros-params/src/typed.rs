//! Typed parameter API
//!
//! This module provides a fluent builder pattern for declaring typed parameters
//! in a ROS 2 node, aligning with the rclrs API.

#[cfg(test)]
use crate::ParameterStorage;
use crate::{
    DeclarationError, NodeKey, ParameterDescriptor, ParameterRange, ParameterServer, ParameterType,
};
use heapless::String;

/// Trait for types that can be used as typed parameters
pub use crate::ParameterVariant;

/// Error type for typed parameter operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterError {
    /// Parameter already declared with a different type
    TypeMismatch,
    /// Value is outside allowed range
    OutOfRange,
    /// Parameter is read-only
    ReadOnly,
    /// Parameter not found
    NotFound,
    /// Internal storage is full
    StorageFull,
    /// String conversion failed
    StringConversion,
    /// Invalid range for type
    InvalidRange,
    /// An on-set-parameters callback refused the write (phase-417 W4.a).
    Rejected,
    /// A set on a name that was never declared, and the server does not
    /// allow undeclared parameters (issue 1151; rclrs `parameter.rs`).
    ///
    /// Distinct from [`NotFound`](Self::NotFound): that is what a DIRECT
    /// `ParameterServer::set` says about a missing name; this is what the
    /// wire-facing [`ParameterServer::apply`](crate::ParameterServerOps::apply)
    /// says when the only remaining option — declaring it — is switched off.
    /// Moved here from the retired `SetParameterResult` (phase-515).
    Undeclared,
}

#[allow(deprecated)]
impl From<crate::SetParameterResult> for ParameterError {
    /// The retired verdict enum's error half, kept for one release beside the
    /// deprecated type (phase-515).
    fn from(result: crate::SetParameterResult) -> Self {
        use crate::SetParameterResult as R;
        match result {
            R::TypeMismatch => ParameterError::TypeMismatch,
            R::OutOfRange => ParameterError::OutOfRange,
            R::ReadOnly => ParameterError::ReadOnly,
            R::NotFound => ParameterError::NotFound,
            R::StorageFull => ParameterError::StorageFull,
            R::InvalidRange => ParameterError::InvalidRange,
            R::Rejected => ParameterError::Rejected,
            R::Undeclared => ParameterError::Undeclared,
            R::Success => panic!("SetParameterResult::Success is not an error"),
        }
    }
}

/// Trait for types that can be converted to a parameter range from `RangeInclusive`
pub trait RangeConvertible: ParameterVariant + Clone {
    /// Convert a `RangeInclusive<Self>` to a `ParameterRange`
    fn to_parameter_range(
        range: core::ops::RangeInclusive<Self>,
    ) -> Result<ParameterRange, ParameterError>;
}

impl RangeConvertible for i64 {
    fn to_parameter_range(
        range: core::ops::RangeInclusive<Self>,
    ) -> Result<ParameterRange, ParameterError> {
        Ok(ParameterRange::Integer(crate::IntegerRange::new(
            *range.start(),
            *range.end(),
            1, // Default step of 1
        )))
    }
}

impl RangeConvertible for f64 {
    fn to_parameter_range(
        range: core::ops::RangeInclusive<Self>,
    ) -> Result<ParameterRange, ParameterError> {
        Ok(ParameterRange::FloatingPoint(
            crate::FloatingPointRange::new(
                *range.start(),
                *range.end(),
                0.0, // No step constraint (any value in range is valid)
            ),
        ))
    }
}

/// Builder for declaring a typed parameter
pub struct ParameterBuilder<'a, 's, T: ParameterVariant> {
    /// Borrow of the parameter server. phase-382 W2' — the server itself
    /// borrows its table, so every handle onto it carries BOTH lifetimes:
    /// `'a` is this borrow, `'s` is the caller-owned storage underneath.
    server: &'a mut ParameterServer<'s>,
    /// phase-426 W1 — the node this parameter belongs to. Fixed when the
    /// builder is made, so a chained call cannot lose it and land the
    /// declaration on a sibling.
    node: NodeKey,
    /// Parameter name
    name: &'a str,
    /// Default value
    default: Option<T>,
    /// Human-readable description
    description: Option<&'a str>,
    /// phase-417 W4.a — the descriptor's other free text,
    /// `rcl_interfaces/msg/ParameterDescriptor::additional_constraints`, which
    /// `ros2 param describe` prints.
    constraints: Option<&'a str>,
    /// Range constraints
    range: Option<ParameterRange>,
    /// Whether the parameter is read-only
    read_only: bool,
    /// Phantom data to hold the type parameter
    _phantom: core::marker::PhantomData<T>,
}

impl<'a, 's, T: ParameterVariant> ParameterBuilder<'a, 's, T> {
    /// Create a new parameter builder
    pub fn new(server: &'a mut ParameterServer<'s>, node: NodeKey, name: &'a str) -> Self {
        Self {
            server,
            node,
            name,
            default: None,
            description: None,
            constraints: None,
            range: None,
            read_only: false,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Set a default value for the parameter
    ///
    /// The ONE default setter. rclrs also has `default_from_iter(impl IntoIterator)`
    /// and `default_string_array(&[&str])`; neither exists here, so pass the array
    /// value itself. An array is a `heapless::Vec` bounded by `MAX_ARRAY_LEN` (a
    /// string array is `Vec<String<MAX_STRING_VALUE_LEN>, MAX_STRING_ARRAY_LEN>`, two
    /// compile-time bounds), and an over-capacity value is refused by
    /// [`ParameterVariant::try_to_parameter_value`], never truncated. No allocator.
    pub fn default(mut self, value: T) -> Self {
        self.default = Some(value);
        self
    }

    /// Set a human-readable description for the parameter
    pub fn description(mut self, desc: &'a str) -> Self {
        self.description = Some(desc);
        self
    }

    /// Set the free-text `additional_constraints` — rclrs's
    /// `ParameterBuilder::constraints`, and rclc's fourth argument to
    /// `rclc_add_parameter_description`.
    ///
    /// Text longer than `NROS_PARAMS_MAX_CONSTRAINTS_BYTES` — whose default is
    /// **0**, so an image pays for this field only when it asks for it — is
    /// truncated at a character boundary and REPORTED once through
    /// [`ParameterServer::take_truncated_descriptions`](crate::ParameterServerOps::take_truncated_descriptions), never dropped in
    /// silence.
    ///
    /// [`ParameterServer::take_truncated_descriptions`]: crate::ParameterServerOps::take_truncated_descriptions
    pub fn constraints(mut self, text: &'a str) -> Self {
        self.constraints = Some(text);
        self
    }

    /// Set integer range constraints for the parameter
    pub fn integer_range(mut self, min: i64, max: i64, step: i64) -> Result<Self, ParameterError> {
        if T::parameter_type() != ParameterType::Integer {
            return Err(ParameterError::InvalidRange);
        }
        let range = crate::IntegerRange::new(min, max, step);
        // Issue 1150 — `min > max` or a negative step is refused here, where
        // the caller can see it, not silently stored and published.
        if !range.is_valid() {
            return Err(ParameterError::InvalidRange);
        }
        self.range = Some(ParameterRange::Integer(range));
        Ok(self)
    }

    /// Set floating point range constraints for the parameter
    pub fn float_range(mut self, min: f64, max: f64, step: f64) -> Result<Self, ParameterError> {
        if T::parameter_type() != ParameterType::Double {
            return Err(ParameterError::InvalidRange);
        }
        let range = crate::FloatingPointRange::new(min, max, step);
        if !range.is_valid() {
            return Err(ParameterError::InvalidRange);
        }
        self.range = Some(ParameterRange::FloatingPoint(range));
        Ok(self)
    }

    /// Set range constraints using an inclusive range
    ///
    /// This is a convenience method that works with `RangeInclusive`:
    /// - For `i64` parameters: `range(0..=100)` sets an integer range with step 1
    /// - For `f64` parameters: `range(0.0..=1.0)` sets a floating point range with step 0.0
    ///
    /// For more control (e.g., custom step), use `integer_range()` or `float_range()`.
    pub fn range(mut self, range: core::ops::RangeInclusive<T>) -> Result<Self, ParameterError>
    where
        T: RangeConvertible,
    {
        self.range = Some(T::to_parameter_range(range)?);
        Ok(self)
    }

    /// The descriptor the three terminal verbs declare with — ONE
    /// construction (phase-417 W4.a).
    ///
    /// It was written out three times, identically but for the `read_only`
    /// line, which is why `additional_constraints` would otherwise have needed
    /// adding in three places and reached whichever two somebody remembered.
    /// `read_only` stays at the call site because that is the one line the
    /// three genuinely disagree about.
    ///
    /// phase-446 F2 -- a too-long description is truncated and reported, never
    /// a reason to refuse the declaration: the contract derives the string
    /// capacity to 0 for a scalar-only image, and this used to fail every
    /// `.description(..)` there with `StringConversion`. The same holds for
    /// the constraints text, whose capacity DEFAULTS to 0.
    fn build_descriptor(&self) -> Result<ParameterDescriptor, DeclarationError> {
        let mut descriptor = ParameterDescriptor::new(self.name, T::parameter_type())
            .ok_or(DeclarationError::NameTooLong)?;
        if let Some(desc) = self.description {
            descriptor.set_description(desc);
        }
        if let Some(text) = self.constraints {
            descriptor.set_additional_constraints(text);
        }
        descriptor.range = self.range.unwrap_or_default();
        Ok(descriptor)
    }

    /// Declare a read-only parameter
    ///
    /// Read-only parameters cannot be changed after declaration.
    /// A default value must be provided.
    pub fn read_only(self) -> Result<ReadOnlyParameter<'a, 's, T>, DeclarationError> {
        // Must have a default value for read-only parameters
        let default_value = self
            .default
            .as_ref()
            .ok_or(DeclarationError::NoValueAvailable)?
            .clone();

        let mut descriptor = self.build_descriptor()?;
        descriptor.read_only = true;

        let param_value = default_value.to_parameter_value();

        declare(
            self.server,
            self.node,
            self.name,
            descriptor,
            Some(&param_value),
        )?;

        Ok(ReadOnlyParameter::new(self.server, self.node, self.name))
    }

    /// Declare a mandatory parameter
    ///
    /// If no default value is provided, it must be set externally before use.
    pub fn mandatory(self) -> Result<MandatoryParameter<'a, 's, T>, DeclarationError> {
        let mut descriptor = self.build_descriptor()?;
        descriptor.read_only = self.read_only;

        let default_value = self.default.map(|v| v.to_parameter_value());

        declare(
            self.server,
            self.node,
            self.name,
            descriptor,
            default_value.as_ref(),
        )?;

        Ok(MandatoryParameter::new(self.server, self.node, self.name))
    }

    /// Declare an optional parameter
    pub fn optional(self) -> Result<OptionalParameter<'a, 's, T>, DeclarationError> {
        let mut descriptor = self.build_descriptor()?;
        descriptor.read_only = self.read_only;

        let default_value = self.default.map(|v| v.to_parameter_value());

        declare(
            self.server,
            self.node,
            self.name,
            descriptor,
            default_value.as_ref(),
        )?;

        Ok(OptionalParameter::new(self.server, self.node, self.name))
    }
}

/// The typed terminals' declaration, reporting rclrs's [`DeclarationError`].
///
/// `ParameterServer::declare_parameter` answers in `ParameterError`, where
/// `TypeMismatch` means "already declared" and `InvalidRange` covers both an
/// ill-formed range and a default outside it; this names each case. It keeps
/// that call (rather than `try_declare_with_descriptor`) because the typed
/// path emits no parameter event on declaration, and changing that is not
/// part of a rename (phase-515 R8).
fn declare(
    server: &mut ParameterServer<'_>,
    node: NodeKey,
    name: &str,
    descriptor: ParameterDescriptor,
    value: Option<&crate::ParameterValue>,
) -> Result<(), DeclarationError> {
    // Issue 2969 -- a name the launch seeded is adopted, not refused: the
    // launch value wins over `value` (the builder's default), as rclrs applies
    // an override at declaration, and must have the declared type.
    if let Some(adopted) =
        server.adopt_launch_seed(node, name, descriptor.param_type, Some(&descriptor))
    {
        return adopted;
    }
    let consistency = ParameterServer::declaration_consistency(
        Some(&descriptor),
        &value.cloned().unwrap_or_default(),
    );
    server
        .declare_parameter(node, name, descriptor, value)
        .map_err(|result| match result {
            ParameterError::TypeMismatch => DeclarationError::AlreadyDeclared,
            ParameterError::InvalidRange => {
                consistency.err().unwrap_or(DeclarationError::InvalidRange)
            }
            _ => DeclarationError::StorageFull,
        })
}

/// A parameter that must always have a value
pub struct MandatoryParameter<'a, 's, T: ParameterVariant> {
    server: &'a mut ParameterServer<'s>,
    /// phase-426 W1 — the node whose table this handle reads and writes.
    node: NodeKey,
    name: String<{ crate::MAX_PARAM_NAME_LEN }>,
    _phantom: core::marker::PhantomData<T>,
}

impl<'a, 's, T: ParameterVariant> MandatoryParameter<'a, 's, T> {
    pub(crate) fn new(server: &'a mut ParameterServer<'s>, node: NodeKey, name: &'a str) -> Self {
        let mut n = String::new();
        n.push_str(name).unwrap();
        Self {
            server,
            node,
            name: n,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Get the current value of the parameter
    pub fn get(&self) -> T {
        self.server
            .get_parameter_value(self.node, self.name.as_str())
            .and_then(|val| T::from_parameter_value(&val))
            .expect("Mandatory parameter must have a value")
    }

    /// Set the value of the parameter
    pub fn set(&mut self, value: T) -> Result<(), ParameterError> {
        // issue 0323 — reject an over-capacity value here rather than letting
        // `to_parameter_value`'s `unwrap_or_default()` turn it into `NotSet`
        // (a TYPE change) or an EMPTY array and then reporting success.
        let converted = value
            .try_to_parameter_value()
            .map_err(|_| ParameterError::StringConversion)?;
        let result = self
            .server
            .set_parameter_value(self.node, self.name.as_str(), converted);
        // The typed API never auto-declares, so an undeclared name is simply
        // not there.
        result.map_err(|e| match e {
            ParameterError::Undeclared => ParameterError::NotFound,
            e => e,
        })
    }
}

/// A parameter that may or may not have a value
pub struct OptionalParameter<'a, 's, T: ParameterVariant> {
    server: &'a mut ParameterServer<'s>,
    /// phase-426 W1 — the node whose table this handle reads and writes.
    node: NodeKey,
    name: String<{ crate::MAX_PARAM_NAME_LEN }>,
    _phantom: core::marker::PhantomData<T>,
}

impl<'a, 's, T: ParameterVariant> OptionalParameter<'a, 's, T> {
    pub(crate) fn new(server: &'a mut ParameterServer<'s>, node: NodeKey, name: &'a str) -> Self {
        let mut n = String::new();
        n.push_str(name).unwrap();
        Self {
            server,
            node,
            name: n,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Get the current value of the parameter, if set
    pub fn get(&self) -> Option<T> {
        self.server
            .get_parameter_value(self.node, self.name.as_str())
            .and_then(|val| T::from_parameter_value(&val))
    }

    /// Set the value of the parameter
    pub fn set(&mut self, value: Option<T>) -> Result<(), ParameterError> {
        let param_value = value.map(|v| v.to_parameter_value());
        let result = self.server.set_parameter_value(
            self.node,
            self.name.as_str(),
            param_value.unwrap_or_default(),
        );
        // The typed API never auto-declares, so an undeclared name is simply
        // not there.
        result.map_err(|e| match e {
            ParameterError::Undeclared => ParameterError::NotFound,
            e => e,
        })
    }
}

/// A parameter whose value cannot be changed after declaration
pub struct ReadOnlyParameter<'a, 's, T: ParameterVariant> {
    server: &'a mut ParameterServer<'s>,
    /// phase-426 W1 — the node whose table this handle reads and writes.
    node: NodeKey,
    name: String<{ crate::MAX_PARAM_NAME_LEN }>,
    _phantom: core::marker::PhantomData<T>,
}

impl<'a, 's, T: ParameterVariant> ReadOnlyParameter<'a, 's, T> {
    pub(crate) fn new(server: &'a mut ParameterServer<'s>, node: NodeKey, name: &'a str) -> Self {
        let mut n = String::new();
        n.push_str(name).unwrap();
        Self {
            server,
            node,
            name: n,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Get the current value of the parameter
    pub fn get(&self) -> T {
        self.server
            .get_parameter_value(self.node, self.name.as_str())
            .and_then(|val| T::from_parameter_value(&val))
            .expect("Read-only parameter must have a value")
    }
}

/// Provides access to undeclared parameters in a ParameterServer
///
/// This struct is returned by `Node::use_undeclared_parameters()` and allows
/// for dynamic retrieval of parameter values by name without explicit declaration.
///
/// rclrs's `Parameters`: read AND write, by name. It was
/// `UndeclaredParameters`, which survives one release as a deprecated alias.
pub struct Parameters<'a, 's> {
    server: &'a mut ParameterServer<'s>,
    /// phase-426 W1 — whose undeclared parameters. Without this the accessor
    /// reads whatever node happened to declare the name first.
    node: NodeKey,
}

/// The former name of [`Parameters`].
// phase-515 R8 took rclrs's name.
#[deprecated(
    since = "0.6.0",
    note = "renamed to `Parameters`, rclrs's name; use `Parameters`"
)]
pub type UndeclaredParameters<'a, 's> = Parameters<'a, 's>;

impl<'a, 's> Parameters<'a, 's> {
    pub fn new(server: &'a mut ParameterServer<'s>, node: NodeKey) -> Self {
        Self { server, node }
    }

    /// rclrs's `Parameters::get::<T>(name)`: the value of
    /// `name` as `T`, or `None` when it is unset or holds another type.
    pub fn get<T: crate::ParameterVariant>(&self, name: &str) -> Option<T> {
        self.server
            .get(self.node, name)
            .and_then(|value| T::from_parameter_value(value))
    }

    /// rclrs's `Parameters::set(name, value)`, declaring
    /// `name` if absent. ADOPT-BOUNDED: it takes `&mut self` where rclrs's
    /// takes `&self` (this store has no interior mutability; the handle
    /// borrows the server mutably), and a value past this node's compile-time
    /// capacity is `ParameterError::StringConversion` rather than rclrs's
    /// `ParameterValueError`.
    pub fn set<T: crate::ParameterVariant>(
        &mut self,
        name: &str,
        value: T,
    ) -> Result<(), ParameterError> {
        let value = value
            .try_to_parameter_value()
            .map_err(|_| ParameterError::StringConversion)?;
        self.set_value(name, value)
    }

    /// Try to get the value of an undeclared boolean parameter
    pub fn get_bool(&self, name: &str) -> Option<bool> {
        self.server.get_bool(self.node, name)
    }

    /// Try to get the value of an undeclared integer parameter
    pub fn get_integer(&self, name: &str) -> Option<i64> {
        self.server.get_integer(self.node, name)
    }

    /// Try to get the value of an undeclared double parameter
    pub fn get_double(&self, name: &str) -> Option<f64> {
        self.server.get_double(self.node, name)
    }

    /// Try to get the value of an undeclared string parameter
    pub fn get_string(&self, name: &str) -> Option<&str> {
        self.server.get_string(self.node, name)
    }

    // ---- writes (phase-417 W4.a) -------------------------------------------
    //
    // rclrs's `Parameters` handle reads AND writes; ours only read, which made
    // `use_undeclared_parameters` half a handle. The writes go through
    // `ParameterServer::set_or_declare` rather than `apply`, and that is the
    // whole point of the type: `apply` refuses a name the node never declared
    // unless the node opted in, and this handle IS the opt-in — a caller
    // reaches it by naming it, exactly as the store's own doc comment for
    // `set_or_declare` describes.

    /// Set (declaring if absent) an undeclared boolean parameter.
    pub fn set_bool(&mut self, name: &str, value: bool) -> Result<(), ParameterError> {
        self.set_value(name, crate::ParameterValue::Bool(value))
    }

    /// Set (declaring if absent) an undeclared integer parameter.
    pub fn set_integer(&mut self, name: &str, value: i64) -> Result<(), ParameterError> {
        self.set_value(name, crate::ParameterValue::Integer(value))
    }

    /// Set (declaring if absent) an undeclared double parameter.
    pub fn set_double(&mut self, name: &str, value: f64) -> Result<(), ParameterError> {
        self.set_value(name, crate::ParameterValue::Double(value))
    }

    /// Set (declaring if absent) an undeclared string parameter.
    pub fn set_string(&mut self, name: &str, value: &str) -> Result<(), ParameterError> {
        let value =
            crate::ParameterValue::from_string(value).ok_or(ParameterError::StringConversion)?;
        self.set_value(name, value)
    }

    fn set_value(
        &mut self,
        name: &str,
        value: crate::ParameterValue,
    ) -> Result<(), ParameterError> {
        self.server
            .set_or_declare(self.node, name, value)
            .map_err(|e| match e {
                ParameterError::Undeclared => ParameterError::NotFound,
                e => e,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// phase-426 W1 — the node these single-node tests are about.
    const NODE: NodeKey = NodeKey::PRIMARY;

    #[test]
    fn test_mandatory_parameter_with_default() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let param = ParameterBuilder::<i64>::new(&mut server, NODE, "test_param")
            .default(42)
            .description("A test parameter")
            .mandatory()
            .expect("Failed to declare parameter");

        assert_eq!(param.get(), 42);
    }

    #[test]
    fn test_mandatory_parameter_set() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let mut param = ParameterBuilder::<i64>::new(&mut server, NODE, "test_param")
            .default(0)
            .mandatory()
            .expect("Failed to declare parameter");

        param.set(100).expect("Failed to set parameter");
        assert_eq!(param.get(), 100);
    }

    #[test]
    fn test_optional_parameter_none() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let param = ParameterBuilder::<i64>::new(&mut server, NODE, "test_param")
            .optional()
            .expect("Failed to declare parameter");

        assert_eq!(param.get(), None);
    }

    #[test]
    fn test_optional_parameter_with_default() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let param = ParameterBuilder::<i64>::new(&mut server, NODE, "test_param")
            .default(42)
            .optional()
            .expect("Failed to declare parameter");

        assert_eq!(param.get(), Some(42));
    }

    #[test]
    fn test_optional_parameter_set() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let mut param = ParameterBuilder::<i64>::new(&mut server, NODE, "test_param")
            .optional()
            .expect("Failed to declare parameter");

        param.set(Some(100)).expect("Failed to set parameter");
        assert_eq!(param.get(), Some(100));
    }

    #[test]
    fn test_read_only_parameter() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let param = ParameterBuilder::<i64>::new(&mut server, NODE, "readonly_param")
            .default(42)
            .description("A read-only parameter")
            .read_only()
            .expect("Failed to declare parameter");

        assert_eq!(param.get(), 42);
    }

    #[test]
    fn test_read_only_parameter_requires_default() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let result = ParameterBuilder::<i64>::new(&mut server, NODE, "readonly_param").read_only();

        assert_eq!(result.err(), Some(DeclarationError::NoValueAvailable));
    }

    /// phase-515 R8 -- the terminals report rclrs's `DeclarationError`, and
    /// each case the old `ParameterError` conflated is named.
    #[test]
    fn builder_terminals_report_declaration_errors() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        ParameterBuilder::<i64>::new(&mut server, NODE, "dup")
            .default(1)
            .mandatory()
            .expect("first declaration");
        let again = ParameterBuilder::<i64>::new(&mut server, NODE, "dup")
            .default(1)
            .mandatory();
        assert_eq!(again.err(), Some(DeclarationError::AlreadyDeclared));

        let outside = ParameterBuilder::<i64>::new(&mut server, NODE, "ranged")
            .default(500)
            .integer_range(0, 100, 1)
            .expect("well-formed range")
            .optional();
        assert_eq!(
            outside.err(),
            Some(DeclarationError::InitialValueOutOfRange)
        );

        // `?` into a `ParameterError` caller still compiles, and reads as before.
        let as_param_error: ParameterError = DeclarationError::AlreadyDeclared.into();
        assert_eq!(as_param_error, ParameterError::TypeMismatch);
    }

    // Issue 2969.
    /// A typed declaration of a name the launch seeded ADOPTS the seed: its
    /// value over the builder's default, checked against the declared type and
    /// range. Only the first declaration adopts; a second one is the code's
    /// own duplicate, refused as before. An unmarked name (no launch seed) is
    /// refused as before too.
    #[test]
    fn a_typed_declaration_adopts_its_launch_seed_once() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        assert!(server.declare_ref(NODE, "period", &crate::ParameterValue::Integer(20)));
        assert!(server.mark_launch_seed(NODE, "period"));
        let period = ParameterBuilder::<i64>::new(&mut server, NODE, "period")
            .default(5)
            .mandatory()
            .expect("the launch seed is adopted");
        assert_eq!(period.get(), 20, "the launch value wins over the default");
        let again = ParameterBuilder::<i64>::new(&mut server, NODE, "period").optional();
        assert_eq!(again.err(), Some(DeclarationError::AlreadyDeclared));

        // Another type: refused naming the cause, never stored over.
        assert!(server.declare_ref(NODE, "gain", &crate::ParameterValue::Integer(3)));
        assert!(server.mark_launch_seed(NODE, "gain"));
        let gain = ParameterBuilder::<f64>::new(&mut server, NODE, "gain").optional();
        assert_eq!(gain.err(), Some(DeclarationError::LaunchValueTypeMismatch));

        // Outside the declared range: refused like an out-of-range default.
        assert!(server.declare_ref(NODE, "rate", &crate::ParameterValue::Integer(500)));
        assert!(server.mark_launch_seed(NODE, "rate"));
        let rate = ParameterBuilder::<i64>::new(&mut server, NODE, "rate")
            .integer_range(0, 100, 1)
            .expect("well-formed range")
            .optional();
        assert_eq!(rate.err(), Some(DeclarationError::InitialValueOutOfRange));

        // No mark: a name already there is a duplicate, as before 2969.
        assert!(server.declare_ref(NODE, "plain", &crate::ParameterValue::Integer(1)));
        let plain = ParameterBuilder::<i64>::new(&mut server, NODE, "plain").optional();
        assert_eq!(plain.err(), Some(DeclarationError::AlreadyDeclared));
    }

    // Issue 3020.
    /// A launch seed is in the store but is not a DECLARATION until the code
    /// declares the name: `is_declared` (what rclcpp's `has_parameter`
    /// answers) is false for it, so Autoware's
    /// `has_parameter(p) ? get_parameter(p) : declare_parameter(p)` still
    /// declares, and the declaration adopts the seed with the code's type. A
    /// seed whose text reads as another type than the declaration is refused,
    /// as rclcpp refuses an integer override for a double parameter, and it
    /// stays undeclared.
    #[test]
    fn a_launch_seed_is_not_declared_until_the_code_declares_it() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());

        assert_eq!(server.seed_launch_text(NODE, "ego_dist", "3.0"), Ok(true));
        assert!(
            !server.is_declared(NODE, "ego_dist"),
            "a seed is not a declaration"
        );
        assert!(
            matches!(server.get(NODE, "ego_dist"), Some(crate::ParameterValue::Double(v)) if *v == 3.0),
            "the seed's value is in the store"
        );
        let dist = ParameterBuilder::<f64>::new(&mut server, NODE, "ego_dist")
            .default(0.0)
            .mandatory()
            .expect("the declaration adopts the seed");
        assert_eq!(dist.get(), 3.0, "the launch value wins over the default");
        assert!(
            server.is_declared(NODE, "ego_dist"),
            "declared once the code declares it"
        );

        // Integer text, double declaration: refused (rclcpp's
        // InvalidParameterTypeException), and still not a declaration.
        assert_eq!(server.seed_launch_text(NODE, "ego_yaw", "1"), Ok(true));
        assert!(!server.is_declared(NODE, "ego_yaw"));
        let yaw = ParameterBuilder::<f64>::new(&mut server, NODE, "ego_yaw").optional();
        assert_eq!(yaw.err(), Some(DeclarationError::LaunchValueTypeMismatch));
        assert!(!server.is_declared(NODE, "ego_yaw"));

        // A name the code declares with no seed is declared; an absent one is not.
        assert!(server.declare_ref(NODE, "plain", &crate::ParameterValue::Integer(1)));
        assert!(server.is_declared(NODE, "plain"));
        assert!(!server.is_declared(NODE, "absent"));
    }

    // Issue 3020.
    /// The refusal names the parameter and both types, and for an integer
    /// under a `double` declaration the exact text to write.
    #[test]
    fn a_launch_type_mismatch_names_the_key_both_types_and_the_fix() {
        use crate::{LaunchTypeMismatch, ParameterType, ParameterValue};
        let line = |declared, launch: &ParameterValue| {
            std::format!(
                "{}",
                LaunchTypeMismatch {
                    name: "ego_nearest_yaw_threshold",
                    declared,
                    launch,
                }
            )
        };
        assert_eq!(
            line(ParameterType::Double, &ParameterValue::Integer(1)),
            "parameter \"ego_nearest_yaw_threshold\" is declared double but the launch value \
             is the integer 1; write 1.0 in the YAML"
        );
        assert_eq!(
            line(ParameterType::Bool, &ParameterValue::Double(0.5)),
            "parameter \"ego_nearest_yaw_threshold\" is declared bool but the launch value \
             is the double 0.5; write a bool value in the YAML"
        );
    }

    // Issue 2992.
    /// A launch seed parsed straight into its slot is the seed the
    /// value-then-mark path made: adopted by the first typed declaration. A
    /// text the store cannot hold, and `[]`, leave the table as it was (no
    /// entry, no count), so the name stays free.
    #[test]
    fn a_launch_seed_from_text_is_parsed_into_its_slot() {
        use crate::LaunchSeedError;
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());

        assert_eq!(server.seed_launch_text(NODE, "period", "20"), Ok(true));
        let period = ParameterBuilder::<i64>::new(&mut server, NODE, "period")
            .default(5)
            .mandatory()
            .expect("the launch seed is adopted");
        assert_eq!(period.get(), 20);

        assert_eq!(
            server.seed_launch_text(NODE, "gains", "[1.5, 2.5]"),
            Ok(true)
        );
        assert!(matches!(
            server.get(NODE, "gains"),
            Some(crate::ParameterValue::DoubleArray(v)) if v.as_slice() == [1.5, 2.5]
        ));

        let before = server.len(NODE);
        assert_eq!(server.seed_launch_text(NODE, "empty", "[]"), Ok(false));
        assert_eq!(
            server.seed_launch_text(NODE, "mixed", "[1, a]"),
            Err(LaunchSeedError::Text(crate::LaunchTextError::MixedList))
        );
        assert_eq!(server.len(NODE), before, "a refused text declares nothing");
        assert!(server.get(NODE, "empty").is_none() && server.get(NODE, "mixed").is_none());
        assert_eq!(server.seed_launch_text(NODE, "mixed", "3"), Ok(true));

        assert_eq!(
            server.seed_launch_text(NODE, "period", "7"),
            Err(LaunchSeedError::Declare(DeclarationError::AlreadyDeclared))
        );
        assert!(matches!(
            server.get(NODE, "period"),
            Some(crate::ParameterValue::Integer(20))
        ));
    }

    #[test]
    fn test_integer_range_constraint() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let mut param = ParameterBuilder::<i64>::new(&mut server, NODE, "ranged_param")
            .default(50)
            .integer_range(0, 100, 1)
            .expect("Failed to set range")
            .mandatory()
            .expect("Failed to declare parameter");

        // Valid value within range
        param.set(75).expect("Failed to set valid value");
        assert_eq!(param.get(), 75);
    }

    #[test]
    fn test_float_range_constraint() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let mut param = ParameterBuilder::<f64>::new(&mut server, NODE, "float_param")
            .default(0.5)
            .float_range(0.0, 1.0, 0.0)
            .expect("Failed to set range")
            .mandatory()
            .expect("Failed to declare parameter");

        // Valid value within range
        param.set(0.75).expect("Failed to set valid value");
        assert_eq!(param.get(), 0.75);
    }

    #[test]
    fn test_range_convenience_integer() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let param = ParameterBuilder::<i64>::new(&mut server, NODE, "ranged_param")
            .default(50)
            .range(0..=100)
            .expect("Failed to set range")
            .mandatory()
            .expect("Failed to declare parameter");

        assert_eq!(param.get(), 50);
    }

    #[test]
    fn test_range_convenience_float() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let param = ParameterBuilder::<f64>::new(&mut server, NODE, "float_param")
            .default(0.5)
            .range(0.0..=1.0)
            .expect("Failed to set range")
            .mandatory()
            .expect("Failed to declare parameter");

        assert_eq!(param.get(), 0.5);
    }

    /// phase-417 W4.a -- the builder's `constraints` reaches the stored
    /// descriptor through the ONE `build_descriptor`, from every terminal
    /// verb, and a text that does not fit the (default 0) capacity is
    /// REPORTED rather than dropped in silence.
    // The comparison and the `min` below read as "always true" to clippy on a
    // build that leaves `NROS_PARAMS_MAX_CONSTRAINTS_BYTES` at its default of 0 --
    // and that is the point: the constant is a KNOB, the test is written
    // against it so it holds at 0, at 64 and at 256, and a version specialised
    // to whichever value this build resolved would read as coverage on the
    // other two.
    #[allow(clippy::absurd_extreme_comparisons, clippy::unnecessary_min_or_max)]
    #[test]
    fn builder_constraints_reach_the_descriptor_from_every_terminal() {
        let text = "positive, hz";
        let fits = text.len() <= crate::MAX_PARAM_CONSTRAINTS_LEN;
        let stored = &text[..crate::MAX_PARAM_CONSTRAINTS_LEN.min(text.len())];

        for which in 0..3u8 {
            let mut storage: ParameterStorage = ParameterStorage::new();
            let mut server = ParameterServer::new_in(storage.as_table());
            {
                let b = ParameterBuilder::<f64>::new(&mut server, NODE, "rate")
                    .default(1.0)
                    .description("publish rate")
                    .constraints(text);
                match which {
                    0 => {
                        b.read_only().expect("read_only");
                    }
                    1 => {
                        b.mandatory().expect("mandatory");
                    }
                    _ => {
                        b.optional().expect("optional");
                    }
                }
            }
            let desc = server.get_descriptor(NODE, "rate").expect("descriptor");
            assert_eq!(desc.description.as_str(), "publish rate");
            assert_eq!(
                desc.additional_constraints.as_str(),
                stored,
                "terminal {which} lost the constraints text"
            );
            let mut reported = 0usize;
            server.take_truncated_descriptions(|_, _, text| {
                // Issue 2686: the constraints field, not the description.
                assert_eq!(text, crate::DescriptorText::Constraints);
                reported += 1;
            });
            assert_eq!(
                reported,
                usize::from(!fits),
                "terminal {which}: a constraints string that did not fit went unreported"
            );
        }
    }

    #[test]
    fn test_parameter_description() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let _param = ParameterBuilder::<i64>::new(&mut server, NODE, "described_param")
            .default(42)
            .description("This is a test description")
            .mandatory()
            .expect("Failed to declare parameter");

        // Verify description was set in the server
        let desc = server.get_descriptor(NODE, "described_param");
        assert!(desc.is_some());
        assert_eq!(
            desc.unwrap().description.as_str(),
            "This is a test description"
        );
    }

    #[test]
    fn test_bool_parameter() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let mut param = ParameterBuilder::<bool>::new(&mut server, NODE, "bool_param")
            .default(false)
            .mandatory()
            .expect("Failed to declare parameter");

        assert!(!param.get());
        param.set(true).expect("Failed to set parameter");
        assert!(param.get());
    }

    #[test]
    fn test_undeclared_parameters() {
        use crate::ParameterValue;

        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());

        // Set some values using set_or_declare (simulating external parameter loading)
        server
            .set_or_declare(NODE, "flag", ParameterValue::Bool(true))
            .unwrap();
        server
            .set_or_declare(NODE, "count", ParameterValue::Integer(42))
            .unwrap();
        server
            .set_or_declare(NODE, "ratio", ParameterValue::Double(0.5))
            .unwrap();

        let undeclared = Parameters::new(&mut server, NODE);

        assert_eq!(undeclared.get_bool("flag"), Some(true));
        assert_eq!(undeclared.get_integer("count"), Some(42));
        assert_eq!(undeclared.get_double("ratio"), Some(0.5));
        assert_eq!(undeclared.get_string("nonexistent"), None);
    }

    /// phase-515 R8 -- rclrs's generic `Parameters::get::<T>` / `set`.
    #[test]
    fn parameters_generic_get_and_set() {
        let mut storage: ParameterStorage = ParameterStorage::new();
        let mut server = ParameterServer::new_in(storage.as_table());
        let mut params = Parameters::new(&mut server, NODE);

        params
            .set("rate", 10i64)
            .expect("set declares an absent name");
        params.set("enabled", true).expect("set a bool");
        assert_eq!(params.get::<i64>("rate"), Some(10));
        assert_eq!(params.get::<bool>("enabled"), Some(true));
        // Another type than the one stored reads as absent, as in rclrs.
        assert_eq!(params.get::<bool>("rate"), None);
        assert_eq!(params.get::<i64>("missing"), None);
        params.set("rate", 20i64).expect("overwrite");
        assert_eq!(params.get::<i64>("rate"), Some(20));
    }
}
