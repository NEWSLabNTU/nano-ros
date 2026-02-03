//! High-level Node API for STM32F4
//!
//! This module provides a simplified interface for ROS-style pub/sub
//! communication without exposing smoltcp or zenoh-pico details.

use core::ffi::{c_char, c_void};

use crate::config::Config;
use crate::platform::Platform;
use crate::{Error, Publisher, Result, Subscriber, SubscriberCallback};

use zenoh_pico_shim_sys::{
    zenoh_shim_close, zenoh_shim_declare_publisher, zenoh_shim_declare_subscriber,
    zenoh_shim_init, zenoh_shim_open, zenoh_shim_spin_once,
};

/// High-level node handle for pub/sub operations
pub struct Node {
    platform: Platform,
    _initialized: bool,
}

impl Node {
    /// Create a publisher for the given topic
    ///
    /// # Arguments
    ///
    /// * `topic` - ROS 2 keyexpr topic (null-terminated, e.g., b"/demo/topic\0")
    ///
    /// # Returns
    ///
    /// A `Publisher` handle that can be used to send messages.
    pub fn create_publisher(&mut self, topic: &[u8]) -> Result<Publisher> {
        let handle = unsafe { zenoh_shim_declare_publisher(topic.as_ptr() as *const c_char) };
        if handle < 0 {
            defmt::error!("Failed to create publisher: {}", handle);
            return Err(Error::Publisher);
        }
        defmt::info!("Publisher created (handle={})", handle);
        Ok(Publisher { handle })
    }

    /// Create a subscriber for the given topic
    ///
    /// # Arguments
    ///
    /// * `topic` - ROS 2 keyexpr topic (null-terminated, e.g., b"/demo/topic\0")
    /// * `callback` - Function to call when a message arrives
    /// * `context` - User context passed to callback
    ///
    /// # Safety
    ///
    /// The callback and context must remain valid for the lifetime of the subscriber.
    pub unsafe fn create_subscriber(
        &mut self,
        topic: &[u8],
        callback: SubscriberCallback,
        context: *mut c_void,
    ) -> Result<Subscriber> {
        let handle = unsafe {
            zenoh_shim_declare_subscriber(topic.as_ptr() as *const c_char, callback, context)
        };
        if handle < 0 {
            defmt::error!("Failed to create subscriber: {}", handle);
            return Err(Error::Subscriber);
        }
        defmt::info!("Subscriber created (handle={})", handle);
        Ok(Subscriber { handle })
    }

    /// Process network events and callbacks
    ///
    /// This must be called periodically to:
    /// - Handle network traffic
    /// - Dispatch subscriber callbacks
    /// - Process zenoh protocol messages
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Maximum time to spend processing (0 = non-blocking)
    pub fn spin_once(&mut self, timeout_ms: u32) {
        // Poll network interface and bridge to zenoh-pico
        self.platform.poll();

        // Process zenoh events
        unsafe {
            zenoh_shim_spin_once(timeout_ms);
        }
    }

    /// Get current uptime in milliseconds
    pub fn now_ms(&self) -> u64 {
        self.platform.now_ms()
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        defmt::info!("Shutting down node...");
        unsafe {
            zenoh_shim_close();
        }
    }
}

/// Entry point for BSP-based applications
///
/// Initializes all hardware and zenoh infrastructure, then calls the
/// user-provided closure with a ready-to-use `Node`.
///
/// # Example
///
/// ```no_run
/// use nano_ros_bsp_stm32f4::prelude::*;
///
/// #[entry]
/// fn main() -> ! {
///     run_node(Config::nucleo_f429zi(), |node| {
///         let pub_ = node.create_publisher(b"0/chatter/std_msgs::msg::dds_::Int32_/TypeHashNotSupported\0")?;
///
///         for i in 0u32..10 {
///             node.spin_once(500);
///             pub_.publish(&i.to_le_bytes())?;
///         }
///         Ok(())
///     })
/// }
/// ```
///
/// # Panics
///
/// Panics if hardware initialization fails.
pub fn run_node<F>(config: Config, f: F) -> !
where
    F: FnOnce(&mut Node) -> Result<()>,
{
    defmt::info!("nano-ros STM32F4 BSP starting...");
    defmt::info!(
        "  IP: {}.{}.{}.{}",
        config.ip[0],
        config.ip[1],
        config.ip[2],
        config.ip[3]
    );

    // Initialize platform (clocks, Ethernet, smoltcp, zenoh-pico platform)
    let platform = match unsafe { crate::platform::init(&config) } {
        Ok(p) => p,
        Err(e) => {
            defmt::error!("Platform init failed: {:?}", e);
            loop {
                cortex_m::asm::wfi();
            }
        }
    };

    // Initialize zenoh session
    defmt::info!("Connecting to zenoh router...");
    let ret = unsafe { zenoh_shim_init(config.zenoh_locator.as_ptr() as *const c_char) };
    if ret < 0 {
        defmt::error!("zenoh_shim_init failed: {}", ret);
        loop {
            cortex_m::asm::wfi();
        }
    }

    let ret = unsafe { zenoh_shim_open() };
    if ret < 0 {
        defmt::error!("zenoh_shim_open failed: {}", ret);
        loop {
            cortex_m::asm::wfi();
        }
    }
    defmt::info!("Zenoh session opened");

    // Create node
    let mut node = Node {
        platform,
        _initialized: true,
    };

    // Run user code
    match f(&mut node) {
        Ok(()) => {
            defmt::info!("Application completed successfully");
        }
        Err(e) => {
            defmt::error!("Application error: {:?}", e);
        }
    }

    // Node will be dropped here, closing zenoh session

    defmt::info!("Entering idle loop");
    loop {
        cortex_m::asm::wfi();
    }
}
