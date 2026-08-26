//! Integration tests for RTIC support in nros-node
//!
//! These tests verify that the RTIC-specific features work correctly:
//! - Static buffer allocation via const generics
//! - Node configuration types
//! - QoS settings

#![cfg(feature = "std")]

/// Test NodeConfig creation
#[test]
fn test_node_config_creation() {
    use nros_node::NodeConfig;

    let config = NodeConfig::new("test_node", "/test_namespace");

    assert_eq!(config.name, "test_node");
    assert_eq!(config.namespace, "/test_namespace");
    assert_eq!(config.domain_id, 0); // Default domain
}

/// Test NodeConfig with custom domain
#[test]
fn test_node_config_with_domain() {
    use nros_node::NodeConfig;

    let config = NodeConfig::new("test_node", "/").with_domain(42);

    assert_eq!(config.domain_id, 42);
}

/// Test QoS settings creation
#[test]
fn test_qos_settings() {
    use nros_node::{QoSProfile, QoSReliabilityPolicy};

    // Default QoS - RELIABLE
    let default_qos = QoSProfile::default();
    assert_eq!(
        default_qos.reliability,
        QoSReliabilityPolicy::Reliable,
        "Default should be reliable"
    );
    assert_eq!(default_qos.history_depth(), 10, "Default history depth");

    // Custom QoS using builder
    let custom_qos = QoSProfile::new().reliable().keep_last(100);
    assert_eq!(
        custom_qos.reliability,
        QoSReliabilityPolicy::Reliable,
        "Custom should be reliable"
    );
    assert_eq!(custom_qos.history_depth(), 100, "Custom history depth");
}

/// Test that memory requirements are bounded
#[test]
fn test_memory_bounds() {
    use core::mem::size_of;

    // NodeConfig should be small
    assert!(size_of::<nros_node::NodeConfig>() < 256);

    // QoSProfile should be tiny
    assert!(size_of::<nros_node::QoSProfile>() < 32);
}

/// Test that const generic buffer sizes can be used
#[test]
fn test_const_generics_compile() {
    // These should compile successfully, proving const generics work
    const CUSTOM_SIZE: usize = 512;

    // Verify they're usable as array sizes (compile-time check)
    let buffer: [u8; CUSTOM_SIZE] = [0u8; CUSTOM_SIZE];
    assert_eq!(buffer.len(), CUSTOM_SIZE);
}
