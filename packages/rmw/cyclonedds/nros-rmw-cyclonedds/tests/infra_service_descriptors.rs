//! Issue 1268 — the services the executor creates ON ITS OWN must have a
//! Cyclone type descriptor.
//!
//! Cyclone creates a topic only for a type it has a registered descriptor for;
//! `create_service` on any other type fails `UNSUPPORTED`. The backend bakes in
//! exactly one descriptor of its own (`ParticipantEntitiesInfo`, for the
//! graph), and a downstream's generated typesupport covers ITS OWN message
//! packages — so nothing covered the six `rcl_interfaces` parameter services,
//! and `ros2 service list` showed none of them for any node of a Cyclone image
//! while the image's own services worked.
//!
//! The executor now registers those types before creating the servers
//! (`create_param_srv`, `nros-node/src/executor/spin.rs`), which is only
//! useful if the builder ACCEPTS them: `Parameter`, `ParameterValue` and
//! `ParameterDescriptor` carry sequences of nested structs, sequences of
//! strings and a bounded sequence, which is the shape class that was rejected
//! outright before phase-212 K.7.4.c. This test builds all twelve.
//!
//! It uses the no-op bridge stub below when `bridge-stub` is off (same shape as
//! `registry_seq_nested.rs`), so it runs in the plain workspace test lane with
//! no feature flags and no `libddsc`. That means it checks the RUST half — the
//! schema walk, nesting depth and field kinds — and not the C++ op emitter.

#[cfg(not(feature = "bridge-stub"))]
use core::ffi::{c_char, c_int, c_void};

use nros_rmw_cyclonedds::dynamic_type::{BuildError, DescriptorBuilder};
use nros_serdes::schema::Message;

#[cfg(not(feature = "bridge-stub"))]
static STUB_BACKING: u8 = 0;

#[cfg(not(feature = "bridge-stub"))]
#[unsafe(no_mangle)]
extern "C" fn nros_cyclonedds_build_descriptor_from_schema(
    _type_name: *const c_char,
    _fields: *const u8,
    _field_count: u32,
    _kinds: *const u8,
    _kind_count: u32,
    _out_err: *mut c_int,
) -> *const c_void {
    &STUB_BACKING as *const u8 as *const c_void
}

#[cfg(not(feature = "bridge-stub"))]
#[unsafe(no_mangle)]
extern "C" fn nros_rmw_cyclonedds_register_descriptor(
    _type_name: *const c_char,
    _descriptor: *const c_void,
) {
}

fn builds<M: Message>() {
    match DescriptorBuilder::build::<M>() {
        Ok(ptr) => assert!(
            !ptr.is_null(),
            "{}: descriptor built but null — Cyclone would refuse the topic",
            M::TYPE_NAME
        ),
        Err(e) => panic!(
            "{}: no descriptor ({e:?}) — `create_service` for a parameter service \
             would fail Unsupported and `ros2 param` would reach no node (issue 1268)",
            M::TYPE_NAME
        ),
    }
}

/// The twelve types the six parameter services put on the wire.
#[test]
fn every_parameter_service_type_builds_a_descriptor() {
    use nros_rcl_interfaces::srv::{
        DescribeParametersRequest, DescribeParametersResponse, GetParameterTypesRequest,
        GetParameterTypesResponse, GetParametersRequest, GetParametersResponse,
        ListParametersRequest, ListParametersResponse, SetParametersAtomicallyRequest,
        SetParametersAtomicallyResponse, SetParametersRequest, SetParametersResponse,
    };

    builds::<GetParametersRequest>();
    builds::<GetParametersResponse>();
    builds::<SetParametersRequest>();
    builds::<SetParametersResponse>();
    builds::<SetParametersAtomicallyRequest>();
    builds::<SetParametersAtomicallyResponse>();
    builds::<ListParametersRequest>();
    builds::<ListParametersResponse>();
    builds::<DescribeParametersRequest>();
    builds::<DescribeParametersResponse>();
    builds::<GetParameterTypesRequest>();
    builds::<GetParameterTypesResponse>();
}

/// Issue 1293 — why the LIFECYCLE services are not fixed by the same change.
///
/// Three of their request types are empty (`FIELDS = &[]`), and the builder
/// refuses an empty schema. ROS pads an empty struct with
/// `uint8 structure_needs_at_least_one_member`, so on the wire it is one byte;
/// our IDL path does that too, and the Rust schema path does not — in EITHER
/// the descriptor or the serializer. Fixing only the descriptor would describe
/// a byte the writer never sends.
///
/// This test states the boundary rather than leaving it to be rediscovered: if
/// someone makes empty schemas build, this fails and points at 1293, which is
/// where the serializer half is written down.
#[test]
fn empty_request_types_have_no_descriptor_yet() {
    use nros_lifecycle_msgs::srv::{
        GetAvailableStatesRequest, GetAvailableTransitionsRequest, GetStateRequest,
    };

    for (name, result) in [
        (
            GetStateRequest::TYPE_NAME,
            DescriptorBuilder::build::<GetStateRequest>(),
        ),
        (
            GetAvailableStatesRequest::TYPE_NAME,
            DescriptorBuilder::build::<GetAvailableStatesRequest>(),
        ),
        (
            GetAvailableTransitionsRequest::TYPE_NAME,
            DescriptorBuilder::build::<GetAvailableTransitionsRequest>(),
        ),
    ] {
        assert!(
            matches!(result, Err(BuildError::EmptySchema)),
            "{name} is empty, so the builder must still refuse it with EmptySchema \
             (issue 1293 carries the descriptor + serializer pair that makes it work); got {result:?}"
        );
    }
}
