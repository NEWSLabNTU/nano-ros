//! Raw CDR payload wrapper for dust-dds TypeSupport.
//!
//! dust-dds requires typed messages implementing `TypeSupport`. Since nros-rmw
//! works with pre-serialized CDR bytes (`publish_raw`/`try_recv_raw`), we need
//! a wrapper type that carries CDR bytes through the DDS pipeline.
//!
//! # Wire Format
//!
//! This type registers as a struct with a single `SEQUENCE<UINT8>` field in the
//! DDS type system. The CDR encoding adds a 4-byte sequence length prefix around
//! the payload. As a result:
//!
//! - **nano-ros ↔ nano-ros DDS**: Works (both sides use the same wrapper)
//! - **nano-ros ↔ ROS 2 DDS**: Not yet compatible (requires typed TypeSupport
//!   matching the actual message layout, planned for 70.11)

use alloc::{string::String, vec::Vec};
use dust_dds::infrastructure::type_support::TypeSupport;
use dust_dds::xtypes::dynamic_type::{
    DynamicData, DynamicDataFactory, DynamicType, DynamicTypeBuilderFactory, ExtensibilityKind,
    MemberDescriptor, TryConstructKind, TypeDescriptor, TypeKind,
};

/// Opaque CDR payload carrier for the DDS typed API.
pub(crate) struct RawCdrPayload {
    pub data: Vec<u8>,
}

impl TypeSupport for RawCdrPayload {
    fn get_type_name() -> &'static str {
        "nros::RawCdrPayload"
    }

    fn get_type() -> DynamicType {
        let uint8_type = DynamicTypeBuilderFactory::get_primitive_type(TypeKind::UINT8);
        let seq_type =
            DynamicTypeBuilderFactory::create_sequence_type(uint8_type, 0 /* unbounded */).build();

        let struct_descriptor = TypeDescriptor {
            kind: TypeKind::STRUCTURE,
            name: String::from("nros::RawCdrPayload"),
            base_type: None,
            discriminator_type: None,
            bound: Vec::new(),
            element_type: None,
            key_element_type: None,
            extensibility_kind: ExtensibilityKind::Final,
            is_nested: false,
        };
        let mut builder = DynamicTypeBuilderFactory::create_type(struct_descriptor);
        let _ = builder.add_member(MemberDescriptor {
            name: String::from("data"),
            r#type: seq_type,
            id: 0,
            default_value: None,
            index: 0,
            label: Vec::new(),
            try_construct_kind: TryConstructKind::UseDefault,
            is_key: false,
            is_optional: false,
            is_must_understand: false,
            is_shared: false,
            is_default_label: false,
        });
        builder.build()
    }

    fn create_sample(src: DynamicData) -> Self {
        let data = src.get_uint8_values(0).unwrap_or_default();
        Self { data }
    }

    fn create_dynamic_sample(self) -> DynamicData {
        let mut dd = DynamicDataFactory::create_data(Self::get_type());
        let _ = dd.set_uint8_values(0, self.data);
        dd
    }
}
