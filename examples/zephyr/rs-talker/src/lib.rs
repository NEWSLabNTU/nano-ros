//! nano-ros Zephyr Talker Example (Rust)
//!
//! This example demonstrates a ROS 2 compatible publisher running on
//! Zephyr RTOS using the nano-ros BSP.
//!
//! Architecture:
//! ```text
//! Rust Application (this file)
//!     └── nano-ros-bsp-zephyr (C BSP)
//!         └── zenoh_shim.c (C shim)
//!             └── zenoh-pico (C library)
//!                 └── Zephyr network stack
//! ```

#![no_std]

use core::ffi::c_char;
use core::marker::PhantomData;

use log::{error, info};

// nano-ros CDR serialization
use nano_ros_core::{RosMessage, Serialize};
use nano_ros_serdes::CdrWriter;

// Generated message types
use std_msgs::msg::Int32;

// =============================================================================
// FFI bindings to nano-ros-bsp-zephyr
// =============================================================================

/// BSP error codes
pub const NANO_ROS_BSP_OK: i32 = 0;

/// Zephyr timeout type (opaque, we just pass k_timeout_t values)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KTimeout {
    ticks: i64,
}

impl KTimeout {
    /// Create a timeout in seconds
    pub const fn secs(s: i64) -> Self {
        // k_ms_to_ticks_ceil64 approximation: assume 1000 ticks/sec for native_sim
        Self { ticks: s * 1000 }
    }
}

#[repr(C)]
pub struct NanoRosBspContext {
    initialized: bool,
    session_open: bool,
}

#[repr(C)]
pub struct NanoRosNode {
    ctx: *mut NanoRosBspContext,
    name: *const c_char,
    domain_id: i32,
}

#[repr(C)]
pub struct NanoRosPublisher {
    node: *mut NanoRosNode,
    handle: i32,
    keyexpr: [u8; 256],
}

unsafe extern "C" {
    fn nano_ros_bsp_init(ctx: *mut NanoRosBspContext) -> i32;
    fn nano_ros_bsp_init_with_locator(ctx: *mut NanoRosBspContext, locator: *const c_char) -> i32;
    fn nano_ros_bsp_shutdown(ctx: *mut NanoRosBspContext);
    fn nano_ros_bsp_is_ready(ctx: *const NanoRosBspContext) -> bool;

    fn nano_ros_bsp_create_node(
        ctx: *mut NanoRosBspContext,
        node: *mut NanoRosNode,
        name: *const c_char,
    ) -> i32;

    fn nano_ros_bsp_create_publisher(
        node: *mut NanoRosNode,
        pub_: *mut NanoRosPublisher,
        topic: *const c_char,
        type_name: *const c_char,
    ) -> i32;

    fn nano_ros_bsp_publish(pub_: *mut NanoRosPublisher, data: *const u8, len: usize) -> i32;

    fn nano_ros_bsp_destroy_publisher(pub_: *mut NanoRosPublisher);

    fn nano_ros_bsp_spin_once(ctx: *mut NanoRosBspContext, timeout: KTimeout) -> i32;
}

// =============================================================================
// High-level Rust API wrapping BSP
// =============================================================================

/// Error type for Zephyr BSP operations
#[derive(Debug, Clone, Copy)]
pub enum BspError {
    /// BSP initialization failed
    InitFailed(i32),
    /// Node creation failed
    NodeFailed(i32),
    /// Publisher creation failed
    PublisherFailed(i32),
    /// Publish failed
    PublishFailed(i32),
    /// Serialization error
    SerializationError,
    /// Topic name too long
    TopicTooLong,
}

/// BSP context wrapper
pub struct BspContext {
    ctx: NanoRosBspContext,
}

impl BspContext {
    /// Initialize BSP with custom locator
    pub fn new(locator: &[u8]) -> Result<Self, BspError> {
        let mut ctx = NanoRosBspContext {
            initialized: false,
            session_open: false,
        };

        let ret = unsafe { nano_ros_bsp_init_with_locator(&mut ctx, locator.as_ptr() as *const c_char) };
        if ret != NANO_ROS_BSP_OK {
            return Err(BspError::InitFailed(ret));
        }

        Ok(Self { ctx })
    }

    /// Check if BSP is ready
    pub fn is_ready(&self) -> bool {
        unsafe { nano_ros_bsp_is_ready(&self.ctx) }
    }

    /// Spin once with timeout
    pub fn spin_once(&mut self, timeout: KTimeout) {
        unsafe {
            nano_ros_bsp_spin_once(&mut self.ctx, timeout);
        }
    }
}

impl Drop for BspContext {
    fn drop(&mut self) {
        unsafe {
            nano_ros_bsp_shutdown(&mut self.ctx);
        }
    }
}

/// BSP Node wrapper
pub struct BspNode<'a> {
    node: NanoRosNode,
    _ctx: PhantomData<&'a mut BspContext>,
}

impl<'a> BspNode<'a> {
    /// Create a new node
    pub fn new(ctx: &'a mut BspContext, name: &[u8]) -> Result<Self, BspError> {
        let mut node = NanoRosNode {
            ctx: &mut ctx.ctx,
            name: core::ptr::null(),
            domain_id: 0,
        };

        let ret = unsafe {
            nano_ros_bsp_create_node(&mut ctx.ctx, &mut node, name.as_ptr() as *const c_char)
        };
        if ret != NANO_ROS_BSP_OK {
            return Err(BspError::NodeFailed(ret));
        }

        Ok(Self {
            node,
            _ctx: PhantomData,
        })
    }

    /// Spin once with timeout
    ///
    /// This processes network events and callbacks.
    pub fn spin_once(&mut self, timeout: KTimeout) {
        unsafe {
            nano_ros_bsp_spin_once(self.node.ctx, timeout);
        }
    }

    /// Create a publisher
    pub fn create_publisher<M: RosMessage>(
        &mut self,
        topic: &[u8],
    ) -> Result<BspPublisher<'a, M>, BspError> {
        let mut pub_ = NanoRosPublisher {
            node: &mut self.node,
            handle: -1,
            keyexpr: [0; 256],
        };

        let type_name = M::TYPE_NAME.as_bytes();
        let mut type_name_buf = [0u8; 128];
        if type_name.len() >= type_name_buf.len() {
            return Err(BspError::TopicTooLong);
        }
        type_name_buf[..type_name.len()].copy_from_slice(type_name);

        let ret = unsafe {
            nano_ros_bsp_create_publisher(
                &mut self.node,
                &mut pub_,
                topic.as_ptr() as *const c_char,
                type_name_buf.as_ptr() as *const c_char,
            )
        };
        if ret != NANO_ROS_BSP_OK {
            return Err(BspError::PublisherFailed(ret));
        }

        Ok(BspPublisher {
            pub_,
            _phantom: PhantomData,
        })
    }
}

/// BSP Publisher wrapper
pub struct BspPublisher<'a, M> {
    pub_: NanoRosPublisher,
    _phantom: PhantomData<&'a M>,
}

impl<M: RosMessage + Serialize> BspPublisher<'_, M> {
    /// Publish a message
    pub fn publish(&mut self, msg: &M) -> Result<(), BspError> {
        let mut buf = [0u8; 256];
        let mut writer = CdrWriter::new(&mut buf);

        msg.serialize(&mut writer)
            .map_err(|_| BspError::SerializationError)?;

        let len = writer.position();
        let ret = unsafe { nano_ros_bsp_publish(&mut self.pub_, buf.as_ptr(), len) };
        if ret != NANO_ROS_BSP_OK {
            return Err(BspError::PublishFailed(ret));
        }

        Ok(())
    }
}

impl<M> Drop for BspPublisher<'_, M> {
    fn drop(&mut self) {
        unsafe {
            nano_ros_bsp_destroy_publisher(&mut self.pub_);
        }
    }
}

// =============================================================================
// Main entry point
// =============================================================================

/// Entry point for Zephyr (called by zephyr-lang-rust)
#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    // Initialize logging
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nano-ros Zephyr Talker (BSP)");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    // Connection parameters (for QEMU/native_sim, connect to host at 192.0.2.2)
    let locator = b"tcp/192.0.2.2:7447\0";

    info!("Connecting to zenoh router...");

    // Initialize BSP
    let mut ctx = match BspContext::new(locator) {
        Ok(ctx) => ctx,
        Err(e) => {
            error!("BSP init failed: {:?}", e);
            return;
        }
    };
    info!("Session opened");

    // Create node
    let mut node = match BspNode::new(&mut ctx, b"talker\0") {
        Ok(n) => n,
        Err(e) => {
            error!("Node creation failed: {:?}", e);
            return;
        }
    };

    // Create publisher
    // Note: Topic should not start with '/' for zenoh keyexpr
    let mut publisher: BspPublisher<Int32> = match node.create_publisher(b"/chatter\0") {
        Ok(p) => p,
        Err(e) => {
            error!("Publisher creation failed: {:?}", e);
            return;
        }
    };

    info!("Publishing messages...");

    // Publish loop
    let mut counter: i32 = 0;

    loop {
        let msg = Int32 { data: counter };

        match publisher.publish(&msg) {
            Ok(()) => {
                info!("[{}] Published: data={}", counter, counter);
            }
            Err(e) => {
                error!("Publish failed: {:?}", e);
            }
        }

        counter = counter.wrapping_add(1);

        // Sleep 1 second
        node.spin_once(KTimeout::secs(1));
    }
}
