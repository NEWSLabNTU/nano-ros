//! ROS 2 Parameter Services
//!
//! This module implements the ROS 2 parameter service interface, enabling
//! `ros2 param` CLI tools to interact with nros nodes.
//!
//! # Services
//!
//! The following services are registered under the node's namespace:
//! - `~/get_parameters` - Get parameter values by name
//! - `~/set_parameters` - Set parameter values
//! - `~/list_parameters` - List all parameters
//! - `~/describe_parameters` - Get parameter descriptors
//! - `~/get_parameter_types` - Get parameter types
//! - `~/set_parameters_atomically` - Set multiple parameters atomically
//!
//! # Example
//!
//! ```ignore
//! use nros::prelude::*;
//!
//! let config = ExecutorConfig::from_env().node_name("my_node");
//! let mut executor: Executor = Executor::open(&config)?;
//! let mut node = executor.create_node("my_node")?;
//!
//! // Parameter services can be integrated via the executor
//! // and respond to `ros2 param list /my_node` etc.
//! ```

// Note: Module is already gated by #[cfg(feature = "param-services")] in lib.rs

extern crate alloc;
use alloc::boxed::Box;

use nros_params::{
    ParameterDescriptor as InternalDescriptor, ParameterServer, ParameterType as InternalType,
    ParameterValue as InternalValue, SetParameterResult,
};

pub use nros_rcl_interfaces::msg::{
    FloatingPointRange, IntegerRange, ListParametersResult, Parameter, ParameterDescriptor,
    ParameterValue, SetParametersResult,
};
pub use nros_rcl_interfaces::srv::{
    DescribeParameters, DescribeParametersRequest, DescribeParametersResponse, GetParameterTypes,
    GetParameterTypesRequest, GetParameterTypesResponse, GetParameters, GetParametersRequest,
    GetParametersResponse, ListParameters, ListParametersRequest, ListParametersResponse,
    SetParameters, SetParametersAtomically, SetParametersAtomicallyRequest,
    SetParametersAtomicallyResponse, SetParametersRequest, SetParametersResponse,
};

/// Maximum number of parameters in a request/response
pub const MAX_PARAMS_PER_REQUEST: usize = 64;

// ═══════════════════════════════════════════════════════════════════════════
// TYPE CONVERSIONS: Internal ↔ nros-rcl-interfaces
// ═══════════════════════════════════════════════════════════════════════════

/// Convert internal ParameterValue to rcl_interfaces ParameterValue
pub fn to_rcl_value(value: &InternalValue) -> ParameterValue {
    let mut result = ParameterValue::default();

    match value {
        InternalValue::NotSet => {
            result.type_ = 0; // PARAMETER_NOT_SET
        }
        InternalValue::Bool(v) => {
            result.type_ = 1; // PARAMETER_BOOL
            result.bool_value = *v;
        }
        InternalValue::Integer(v) => {
            result.type_ = 2; // PARAMETER_INTEGER
            result.integer_value = *v;
        }
        InternalValue::Double(v) => {
            result.type_ = 3; // PARAMETER_DOUBLE
            result.double_value = *v;
        }
        InternalValue::String(v) => {
            result.type_ = 4; // PARAMETER_STRING
            let _ = result.string_value.push_str(v.as_str());
        }
        InternalValue::ByteArray(v) => {
            result.type_ = 5; // PARAMETER_BYTE_ARRAY
            for &b in v.iter() {
                let _ = result.byte_array_value.push(b);
            }
        }
        InternalValue::BoolArray(v) => {
            result.type_ = 6; // PARAMETER_BOOL_ARRAY
            for &b in v.iter() {
                let _ = result.bool_array_value.push(b);
            }
        }
        InternalValue::IntegerArray(v) => {
            result.type_ = 7; // PARAMETER_INTEGER_ARRAY
            for &i in v.iter() {
                let _ = result.integer_array_value.push(i);
            }
        }
        InternalValue::DoubleArray(v) => {
            result.type_ = 8; // PARAMETER_DOUBLE_ARRAY
            for &d in v.iter() {
                let _ = result.double_array_value.push(d);
            }
        }
        InternalValue::StringArray(v) => {
            result.type_ = 9; // PARAMETER_STRING_ARRAY
            for s in v.iter() {
                let mut hs = heapless::String::new();
                let _ = hs.push_str(s.as_str());
                let _ = result.string_array_value.push(hs);
            }
        }
    }

    result
}

/// Convert rcl_interfaces ParameterValue to internal ParameterValue
pub fn from_rcl_value(value: &ParameterValue) -> InternalValue {
    match value.type_ {
        0 => InternalValue::NotSet,
        1 => InternalValue::Bool(value.bool_value),
        2 => InternalValue::Integer(value.integer_value),
        3 => InternalValue::Double(value.double_value),
        4 => {
            let mut s = heapless::String::new();
            let _ = s.push_str(value.string_value.as_str());
            InternalValue::String(s)
        }
        5 => {
            let mut v = heapless::Vec::new();
            for &b in value.byte_array_value.iter() {
                let _ = v.push(b);
            }
            InternalValue::ByteArray(v)
        }
        6 => {
            let mut v = heapless::Vec::new();
            for &b in value.bool_array_value.iter() {
                let _ = v.push(b);
            }
            InternalValue::BoolArray(v)
        }
        7 => {
            let mut v = heapless::Vec::new();
            for &i in value.integer_array_value.iter() {
                let _ = v.push(i);
            }
            InternalValue::IntegerArray(v)
        }
        8 => {
            let mut v = heapless::Vec::new();
            for &d in value.double_array_value.iter() {
                let _ = v.push(d);
            }
            InternalValue::DoubleArray(v)
        }
        9 => {
            let mut v = heapless::Vec::new();
            for s in value.string_array_value.iter() {
                let mut hs = heapless::String::new();
                let _ = hs.push_str(s.as_str());
                let _ = v.push(hs);
            }
            InternalValue::StringArray(v)
        }
        _ => InternalValue::NotSet,
    }
}

/// Convert internal ParameterType to u8 type code
pub fn type_to_u8(param_type: InternalType) -> u8 {
    match param_type {
        InternalType::NotSet => 0,
        InternalType::Bool => 1,
        InternalType::Integer => 2,
        InternalType::Double => 3,
        InternalType::String => 4,
        InternalType::ByteArray => 5,
        InternalType::BoolArray => 6,
        InternalType::IntegerArray => 7,
        InternalType::DoubleArray => 8,
        InternalType::StringArray => 9,
    }
}

/// Convert internal ParameterDescriptor to rcl_interfaces ParameterDescriptor
pub fn to_rcl_descriptor(desc: &InternalDescriptor) -> ParameterDescriptor {
    let mut result = ParameterDescriptor::default();

    let _ = result.name.push_str(desc.name.as_str());
    result.type_ = type_to_u8(desc.param_type);
    let _ = result.description.push_str(desc.description.as_str());
    result.read_only = desc.read_only;
    result.dynamic_typing = desc.dynamic_typing;

    // Convert range constraints
    match &desc.range {
        nros_params::ParameterRange::None => {}
        nros_params::ParameterRange::Integer(range) => {
            let ir = IntegerRange {
                from_value: range.min,
                to_value: range.max,
                step: range.step as u64,
            };
            let _ = result.integer_range.push(ir);
        }
        nros_params::ParameterRange::FloatingPoint(range) => {
            let fr = FloatingPointRange {
                from_value: range.min,
                to_value: range.max,
                step: range.step,
            };
            let _ = result.floating_point_range.push(fr);
        }
    }

    result
}

/// Convert SetParameterResult to rcl_interfaces SetParametersResult
pub fn to_rcl_set_result(result: SetParameterResult) -> SetParametersResult {
    let reason = match result {
        SetParameterResult::Success => "",
        SetParameterResult::ReadOnly => "Parameter is read-only",
        SetParameterResult::TypeMismatch => "Type mismatch",
        SetParameterResult::OutOfRange => "Value out of range",
        SetParameterResult::NotFound => "Parameter not found",
        SetParameterResult::StorageFull => "Parameter storage full",
    };
    let mut reason_str = heapless::String::new();
    let _ = reason_str.push_str(reason);
    SetParametersResult {
        successful: result.is_success(),
        reason: reason_str,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SERVICE HANDLERS
// ═══════════════════════════════════════════════════════════════════════════

/// Handle GetParameters service request
///
/// Returns `Box<Response>` because the response type contains large heapless arrays
/// (~1MB+ for Vec<ParameterValue, 64>) that would overflow the stack.
pub fn handle_get_parameters(
    server: &ParameterServer,
    request: &GetParametersRequest,
) -> Box<GetParametersResponse> {
    let mut response = Box::new(GetParametersResponse::default());

    for name in request.names.iter() {
        let value = server
            .get(name.as_str())
            .map(to_rcl_value)
            .unwrap_or_else(|| {
                // Return NOT_SET for unknown parameters
                ParameterValue {
                    type_: 0,
                    ..Default::default()
                }
            });
        let _ = response.values.push(value);
    }

    response
}

/// Handle SetParameters service request
///
/// Returns `Box<Response>` because request/response types contain large heapless arrays.
pub fn handle_set_parameters(
    server: &mut ParameterServer,
    request: &SetParametersRequest,
) -> Box<SetParametersResponse> {
    let mut response = Box::new(SetParametersResponse::default());

    for param in request.parameters.iter() {
        let value = from_rcl_value(&param.value);
        let result = if server.has(param.name.as_str()) {
            server.set(param.name.as_str(), value)
        } else {
            // Try to declare new parameter
            if server.declare(param.name.as_str(), value) {
                SetParameterResult::Success
            } else {
                SetParameterResult::StorageFull
            }
        };
        let _ = response.results.push(to_rcl_set_result(result));
    }

    response
}

/// Handle SetParametersAtomically service request
///
/// Returns `Box<Response>` because request/response types contain large heapless arrays.
pub fn handle_set_parameters_atomically(
    server: &mut ParameterServer,
    request: &SetParametersAtomicallyRequest,
) -> Box<SetParametersAtomicallyResponse> {
    let mut response = Box::new(SetParametersAtomicallyResponse::default());

    // First, validate all parameters can be set
    let mut can_set_all = true;
    for param in request.parameters.iter() {
        let value = from_rcl_value(&param.value);

        // Check if setting would succeed
        if server.has(param.name.as_str()) {
            // Check read-only and type constraints
            if let Some(desc) = server.get_descriptor(param.name.as_str()) {
                if desc.read_only {
                    can_set_all = false;
                    break;
                }
                if !desc.dynamic_typing && desc.param_type != value.param_type() {
                    can_set_all = false;
                    break;
                }
                if !desc.validate_range(&value) {
                    can_set_all = false;
                    break;
                }
            }
        } else if server.is_full() {
            can_set_all = false;
            break;
        }
    }

    if can_set_all {
        // Set all parameters
        for param in request.parameters.iter() {
            let value = from_rcl_value(&param.value);
            if server.has(param.name.as_str()) {
                let _ = server.set(param.name.as_str(), value);
            } else {
                let _ = server.declare(param.name.as_str(), value);
            }
        }
        response.result.successful = true;
    } else {
        response.result.successful = false;
        let _ = response
            .result
            .reason
            .push_str("One or more parameters could not be set");
    }

    response
}

/// Handle ListParameters service request
///
/// Returns `Box<Response>` because response type contains large heapless arrays (~32KB).
pub fn handle_list_parameters(
    server: &ParameterServer,
    request: &ListParametersRequest,
) -> Box<ListParametersResponse> {
    let mut response = Box::new(ListParametersResponse::default());

    // Collect parameter names
    let mut names: heapless::Vec<heapless::String<256>, MAX_PARAMS_PER_REQUEST> =
        heapless::Vec::new();
    let mut prefixes: heapless::Vec<heapless::String<256>, MAX_PARAMS_PER_REQUEST> =
        heapless::Vec::new();

    for param in server.iter() {
        let name = param.name.as_str();

        // Check prefix filter
        let matches_prefix = if request.prefixes.is_empty() {
            true
        } else {
            request
                .prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix.as_str()))
        };

        if matches_prefix {
            // Check depth filter (0 = unlimited)
            let should_include = if request.depth == 0 {
                true
            } else {
                // Count dots in the name after the prefix
                let depth = name.matches('.').count() as u64;
                depth <= request.depth
            };

            if should_include {
                let mut n = heapless::String::new();
                let _ = n.push_str(name);
                let _ = names.push(n);

                // Extract prefix (everything up to the last dot)
                if let Some(dot_pos) = name.rfind('.') {
                    let prefix = &name[..dot_pos];
                    let mut p = heapless::String::new();
                    let _ = p.push_str(prefix);
                    // Only add if not already present
                    if !prefixes.iter().any(|existing| existing.as_str() == prefix) {
                        let _ = prefixes.push(p);
                    }
                }
            }
        }
    }

    response.result.names = names;
    response.result.prefixes = prefixes;

    response
}

/// Handle DescribeParameters service request
///
/// Returns `Box<Response>` because response type contains large heapless arrays (~50KB).
pub fn handle_describe_parameters(
    server: &ParameterServer,
    request: &DescribeParametersRequest,
) -> Box<DescribeParametersResponse> {
    let mut response = Box::new(DescribeParametersResponse::default());

    for name in request.names.iter() {
        let descriptor = if let Some(desc) = server.get_descriptor(name.as_str()) {
            to_rcl_descriptor(desc)
        } else if let Some(param) = server.get_parameter(name.as_str()) {
            // Create a minimal descriptor from the parameter
            let mut d = ParameterDescriptor::default();
            let _ = d.name.push_str(name.as_str());
            d.type_ = type_to_u8(param.param_type());
            d
        } else {
            // Unknown parameter - return empty descriptor
            let mut d = ParameterDescriptor::default();
            let _ = d.name.push_str(name.as_str());
            d.type_ = 0; // NOT_SET
            d
        };
        let _ = response.descriptors.push(descriptor);
    }

    response
}

/// Handle GetParameterTypes service request
///
/// Returns `Box<Response>` for API consistency with other handlers.
pub fn handle_get_parameter_types(
    server: &ParameterServer,
    request: &GetParameterTypesRequest,
) -> Box<GetParameterTypesResponse> {
    let mut response = Box::new(GetParameterTypesResponse::default());

    for name in request.names.iter() {
        let type_code = server.get_type(name.as_str()).map(type_to_u8).unwrap_or(0); // NOT_SET for unknown
        let _ = response.types.push(type_code);
    }

    response
}

// ═══════════════════════════════════════════════════════════════════════════
// PARAMETER SERVICE SERVERS
// ═══════════════════════════════════════════════════════════════════════════

use crate::executor::{EmbeddedServiceServer, NodeError};

// PARAM_SERVICE_BUFFER_SIZE is generated by build.rs from the
// NROS_PARAM_SERVICE_BUFFER_SIZE env var (default 4096).
pub use crate::config::PARAM_SERVICE_BUFFER_SIZE;

/// Type alias for a parameter service server with standard buffer sizes.
type ParamServer<Svc> =
    EmbeddedServiceServer<Svc, PARAM_SERVICE_BUFFER_SIZE, PARAM_SERVICE_BUFFER_SIZE>;

/// Holds the 6 ROS 2 parameter service servers for a node.
///
/// These servers handle `ros2 param` CLI interactions:
/// - `get_parameters` / `set_parameters` / `set_parameters_atomically`
/// - `list_parameters` / `describe_parameters` / `get_parameter_types`
///
/// Boxed when stored in executor to avoid 48KB+ on the stack
/// (6 servers × 8KB buffers each).
pub struct ParameterServiceServers {
    get_parameters: ParamServer<GetParameters>,
    set_parameters: ParamServer<SetParameters>,
    set_parameters_atomically: ParamServer<SetParametersAtomically>,
    list_parameters: ParamServer<ListParameters>,
    describe_parameters: ParamServer<DescribeParameters>,
    get_parameter_types: ParamServer<GetParameterTypes>,
}

impl ParameterServiceServers {
    /// Create a new set of parameter service servers
    pub(crate) fn new(
        get_parameters: ParamServer<GetParameters>,
        set_parameters: ParamServer<SetParameters>,
        set_parameters_atomically: ParamServer<SetParametersAtomically>,
        list_parameters: ParamServer<ListParameters>,
        describe_parameters: ParamServer<DescribeParameters>,
        get_parameter_types: ParamServer<GetParameterTypes>,
    ) -> Self {
        Self {
            get_parameters,
            set_parameters,
            set_parameters_atomically,
            list_parameters,
            describe_parameters,
            get_parameter_types,
        }
    }

    /// Process all parameter service servers, handling any pending requests.
    ///
    /// Uses split borrows: the `server` parameter provides mutable access to the
    /// `ParameterServer` while `self` provides access to the service servers.
    ///
    /// Returns the number of requests handled.
    pub(crate) fn process(&mut self, server: &mut ParameterServer) -> Result<usize, NodeError> {
        let mut count = 0;

        if self
            .get_parameters
            .handle_request_boxed(|req| handle_get_parameters(server, req))?
        {
            count += 1;
        }

        if self
            .set_parameters
            .handle_request_boxed(|req| handle_set_parameters(server, req))?
        {
            count += 1;
        }

        if self
            .set_parameters_atomically
            .handle_request_boxed(|req| handle_set_parameters_atomically(server, req))?
        {
            count += 1;
        }

        if self
            .list_parameters
            .handle_request_boxed(|req| handle_list_parameters(server, req))?
        {
            count += 1;
        }

        if self
            .describe_parameters
            .handle_request_boxed(|req| handle_describe_parameters(server, req))?
        {
            count += 1;
        }

        if self
            .get_parameter_types
            .handle_request_boxed(|req| handle_get_parameter_types(server, req))?
        {
            count += 1;
        }

        Ok(count)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TYPE-ERASED PARAMETER PROCESSING (for Executor integration)
// ═══════════════════════════════════════════════════════════════════════════

/// Type-erased trait for processing parameter services inside `spin_once()`.
///
/// The concrete `ParameterServiceServers` is stored behind a
/// `Box<dyn ParamServiceProcessor>` so the executor can call `process()`
/// without coupling to the parameter service implementation.
pub(crate) trait ParamServiceProcessor {
    fn process_services(&mut self, server: &mut ParameterServer) -> Result<usize, NodeError>;
}

impl ParamServiceProcessor for ParameterServiceServers {
    fn process_services(&mut self, server: &mut ParameterServer) -> Result<usize, NodeError> {
        self.process(server)
    }
}

/// Holds parameter server state for the executor.
///
/// Stored outside the arena so it doesn't consume `MAX_CBS` slots.
pub(crate) struct ParamState {
    pub(crate) server: ParameterServer,
    pub(crate) services: Box<dyn ParamServiceProcessor>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_conversion_bool() {
        let internal = InternalValue::Bool(true);
        let rcl = to_rcl_value(&internal);
        assert_eq!(rcl.type_, 1);
        assert!(rcl.bool_value);

        let back = from_rcl_value(&rcl);
        assert_eq!(back.as_bool(), Some(true));
    }

    #[test]
    fn test_value_conversion_integer() {
        let internal = InternalValue::Integer(42);
        let rcl = to_rcl_value(&internal);
        assert_eq!(rcl.type_, 2);
        assert_eq!(rcl.integer_value, 42);

        let back = from_rcl_value(&rcl);
        assert_eq!(back.as_integer(), Some(42));
    }

    #[test]
    fn test_value_conversion_double() {
        let internal = InternalValue::Double(3.14);
        let rcl = to_rcl_value(&internal);
        assert_eq!(rcl.type_, 3);
        assert!((rcl.double_value - 3.14).abs() < 0.001);

        let back = from_rcl_value(&rcl);
        assert!((back.as_double().unwrap() - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_value_conversion_string() {
        let internal = InternalValue::from_string("hello").unwrap();
        let rcl = to_rcl_value(&internal);
        assert_eq!(rcl.type_, 4);
        assert_eq!(rcl.string_value.as_str(), "hello");

        let back = from_rcl_value(&rcl);
        assert_eq!(back.as_string(), Some("hello"));
    }

    #[test]
    fn test_get_parameters_handler() {
        use alloc::boxed::Box;

        let mut server = ParameterServer::new();
        server.declare("speed", InternalValue::Double(1.0));
        server.declare("enabled", InternalValue::Bool(true));

        // Use Box for request due to large heapless::Vec size (~1MB+)
        // Handler returns Box<Response> internally
        let mut request = Box::new(GetParametersRequest::default());
        let mut n1 = heapless::String::new();
        n1.push_str("speed").unwrap();
        let mut n2 = heapless::String::new();
        n2.push_str("enabled").unwrap();
        request.names.push(n1).unwrap();
        request.names.push(n2).unwrap();

        let response = handle_get_parameters(&server, &request);
        assert_eq!(response.values.len(), 2);
        assert_eq!(response.values[0].type_, 3); // DOUBLE
        assert_eq!(response.values[1].type_, 1); // BOOL
    }

    #[test]
    fn test_set_parameters_handler() {
        use alloc::boxed::Box;

        let mut server = ParameterServer::new();
        server.declare("speed", InternalValue::Double(1.0));

        // Use Box for request due to large heapless::Vec size (~1MB+)
        // Handler returns Box<Response> internally
        let mut request = Box::new(SetParametersRequest::default());
        let mut param = Box::new(Parameter::default());
        param.name.push_str("speed").unwrap();
        param.value.type_ = 3; // DOUBLE
        param.value.double_value = 2.5;
        request.parameters.push(*param).unwrap();

        let response = handle_set_parameters(&mut server, &request);
        assert_eq!(response.results.len(), 1);
        assert!(response.results[0].successful);

        assert_eq!(server.get_double("speed"), Some(2.5));
    }

    #[test]
    fn test_list_parameters_handler() {
        use alloc::boxed::Box;

        let mut server = ParameterServer::new();
        server.declare("robot.speed", InternalValue::Double(1.0));
        server.declare("robot.name", InternalValue::from_string("bot1").unwrap());
        server.declare("sensor.range", InternalValue::Double(10.0));

        // Use Box for request due to large heapless::Vec size
        let request = Box::new(ListParametersRequest::default());
        let response = handle_list_parameters(&server, &request);

        assert_eq!(response.result.names.len(), 3);
    }

    #[test]
    fn test_get_parameter_types_handler() {
        use alloc::boxed::Box;

        let mut server = ParameterServer::new();
        server.declare("speed", InternalValue::Double(1.0));
        server.declare("count", InternalValue::Integer(5));

        // Use Box for request due to large heapless::Vec size
        let mut request = Box::new(GetParameterTypesRequest::default());
        let mut n1 = heapless::String::new();
        n1.push_str("speed").unwrap();
        let mut n2 = heapless::String::new();
        n2.push_str("count").unwrap();
        request.names.push(n1).unwrap();
        request.names.push(n2).unwrap();

        let response = handle_get_parameter_types(&server, &request);
        assert_eq!(response.types.len(), 2);
        assert_eq!(response.types[0], 3); // DOUBLE
        assert_eq!(response.types[1], 2); // INTEGER
    }
}
