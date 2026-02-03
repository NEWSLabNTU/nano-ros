//! nano-ros Zephyr Action Client Example (Rust)
//!
//! This example demonstrates a ROS 2 compatible action client running on
//! Zephyr RTOS using the zenoh-pico-shim crate.
//!
//! The client sends a Fibonacci goal and receives feedback as the sequence
//! is computed.
//!
//! Architecture:
//! ```text
//! Rust Application (this file)
//!     └── zenoh-pico-shim (Rust wrapper)
//!         └── zenoh_shim.c (C shim, compiled by Zephyr)
//!             └── zenoh-pico (C library)
//!                 └── Zephyr network stack
//! ```
//!
//! Action channels used:
//! - send_goal (query) - Submit goal and get acceptance response
//! - feedback (subscriber) - Receive progress updates

#![no_std]

use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, Ordering};

use log::{error, info, warn};

// nano-ros CDR serialization
use nano_ros_core::{Deserialize, RosMessage, Serialize};
use nano_ros_serdes::{CdrReader, CdrWriter};

// zenoh-pico shim crate
use zenoh_pico_shim::{ShimCallback, ShimContext, ShimError, ShimSubscriber};

// Generated action types
use example_interfaces::action::{FibonacciFeedback, FibonacciGoal};

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Clone, Copy)]
pub enum ActionError {
    Shim(ShimError),
    SerializationError,
    DeserializationError,
    KeyExprTooLong,
    Timeout,
    Rejected,
}

impl From<ShimError> for ActionError {
    fn from(err: ShimError) -> Self {
        ActionError::Shim(err)
    }
}

// =============================================================================
// Feedback Handler
// =============================================================================

/// Static storage for received feedback
static mut LAST_FEEDBACK: Option<FibonacciFeedback> = None;
static FEEDBACK_COUNT: AtomicU32 = AtomicU32::new(0);

/// Expected goal ID (set when goal is sent)
static mut EXPECTED_GOAL_ID: [u8; 16] = [0u8; 16];

/// Callback for feedback messages
extern "C" fn on_feedback(
    _keyexpr: *const i8,
    _keyexpr_len: usize,
    payload: *const u8,
    payload_len: usize,
    _ctx: *mut c_void,
) {
    let count = FEEDBACK_COUNT.fetch_add(1, Ordering::Relaxed);

    // Safety: payload is valid for payload_len bytes
    let payload_slice = if payload.is_null() || payload_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(payload, payload_len) }
    };

    // Deserialize feedback message (goal_id + feedback)
    let mut reader = CdrReader::new(payload_slice);

    // Read goal_id (UUID as sequence)
    let mut goal_id = [0u8; 16];
    let uuid_len = match reader.read_u32() {
        Ok(len) => len as usize,
        Err(_) => {
            info!("[FB {}] Failed to read UUID length", count);
            return;
        }
    };
    if uuid_len != 16 {
        info!("[FB {}] Invalid UUID length: {}", count, uuid_len);
        return;
    }
    for i in 0..16 {
        goal_id[i] = match reader.read_u8() {
            Ok(b) => b,
            Err(_) => {
                info!("[FB {}] Failed to read UUID byte", count);
                return;
            }
        };
    }

    // Check if this feedback is for our goal
    let matches = unsafe { goal_id == EXPECTED_GOAL_ID };
    if !matches {
        info!("[FB {}] Feedback for different goal, ignoring", count);
        return;
    }

    // Read feedback
    let feedback = match FibonacciFeedback::deserialize(&mut reader) {
        Ok(f) => f,
        Err(_) => {
            info!("[FB {}] Failed to deserialize feedback", count);
            return;
        }
    };

    info!(
        "Feedback #{}: {:?}",
        count + 1,
        feedback.sequence.as_slice()
    );

    // Store feedback
    unsafe {
        LAST_FEEDBACK = Some(feedback);
    }
}

// =============================================================================
// Key Expression Constants
// =============================================================================

const SEND_GOAL_KEYEXPR: &[u8] = b"demo/fibonacci/_action/send_goal\0";
const FEEDBACK_KEYEXPR: &[u8] = b"demo/fibonacci/_action/feedback\0";

// =============================================================================
// Helper Functions
// =============================================================================

/// Generate a simple pseudo-random goal ID
fn generate_goal_id(seed: u32) -> [u8; 16] {
    let mut id = [0u8; 16];
    // Simple LCG for pseudo-random bytes
    let mut state = seed.wrapping_mul(1103515245).wrapping_add(12345);
    for byte in &mut id {
        state = state.wrapping_mul(1103515245).wrapping_add(12345);
        *byte = (state >> 16) as u8;
    }
    id
}

/// Send a goal and wait for acceptance
fn send_goal(
    ctx: &ShimContext,
    goal: &FibonacciGoal,
    goal_id: &[u8; 16],
) -> Result<bool, ActionError> {
    // Serialize request (goal_id + goal)
    let mut request_buf = [0u8; 256];
    let mut writer = CdrWriter::new(&mut request_buf);

    // Write goal_id (UUID as sequence)
    writer
        .write_u32(16)
        .map_err(|_| ActionError::SerializationError)?;
    for b in goal_id {
        writer
            .write_u8(*b)
            .map_err(|_| ActionError::SerializationError)?;
    }

    // Write goal
    goal.serialize(&mut writer)
        .map_err(|_| ActionError::SerializationError)?;

    let request_len = writer.position();

    // Send query and wait for reply
    let mut reply_buf = [0u8; 64];
    let reply_len = ctx
        .get(SEND_GOAL_KEYEXPR, &request_buf[..request_len], &mut reply_buf, 10000)
        .map_err(|e| match e {
            ShimError::Timeout => ActionError::Timeout,
            other => ActionError::Shim(other),
        })?;

    // Deserialize response (accepted: bool, stamp: Time)
    let mut reader = CdrReader::new(&reply_buf[..reply_len]);

    // Read accepted (bool as u8)
    let accepted = reader
        .read_u8()
        .map_err(|_| ActionError::DeserializationError)?
        != 0;

    // Read stamp (Time: sec, nanosec) - we don't need it
    let _ = reader.read_i32();
    let _ = reader.read_u32();

    Ok(accepted)
}

// =============================================================================
// Main Entry Point
// =============================================================================

#[no_mangle]
extern "C" fn rust_main() {
    // Initialize logging
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nano-ros Zephyr Action Client Starting");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);
    info!("Action: Fibonacci");

    // Connection parameters
    let locator = b"tcp/192.0.2.2:7447\0";
    info!("Connecting to zenoh router at tcp/192.0.2.2:7447");

    // Create ShimContext
    let ctx = match ShimContext::new(locator) {
        Ok(ctx) => ctx,
        Err(e) => {
            error!("Failed to create context: {}", e);
            return;
        }
    };
    info!("Session opened");

    // Subscribe to feedback
    info!("Subscribing to feedback topic...");
    let _feedback_subscriber = match unsafe {
        ctx.declare_subscriber_raw(FEEDBACK_KEYEXPR, on_feedback as ShimCallback, core::ptr::null_mut())
    } {
        Ok(s) => {
            info!("Feedback subscriber ready");
            s
        }
        Err(e) => {
            error!("Failed to subscribe to feedback: {}", e);
            return;
        }
    };

    // Allow time for connection to stabilize
    info!("Waiting for server...");
    zephyr::time::sleep(zephyr::time::Duration::secs(3));

    // Generate a goal ID
    let goal_id = generate_goal_id(12345);
    unsafe {
        EXPECTED_GOAL_ID = goal_id;
    }

    // Create goal - compute Fibonacci sequence up to order 10
    let goal = FibonacciGoal { order: 10 };
    info!("Sending goal: order={}", goal.order);
    info!(
        "Goal ID: {:02x}{:02x}{:02x}{:02x}...",
        goal_id[0], goal_id[1], goal_id[2], goal_id[3]
    );

    // Send goal
    match send_goal(&ctx, &goal, &goal_id) {
        Ok(true) => {
            info!("Goal accepted!");
        }
        Ok(false) => {
            warn!("Goal was rejected by the server");
            return;
        }
        Err(ActionError::Timeout) => {
            error!("Timeout waiting for goal response");
            error!("Make sure the action server is running");
            return;
        }
        Err(e) => {
            error!("Failed to send goal: {:?}", e);
            return;
        }
    }

    info!("Waiting for feedback and result...");

    // Wait for feedback - Fibonacci(10) takes about 5.5 seconds (11 iterations * 500ms each)
    let mut last_feedback_count = 0u32;
    let mut no_feedback_cycles = 0u32;
    let max_wait_cycles = 200; // 20 seconds max (100ms per cycle)

    for cycle in 0..max_wait_cycles {
        zephyr::time::sleep(zephyr::time::Duration::millis(100));

        let current_count = FEEDBACK_COUNT.load(Ordering::Relaxed);

        if current_count > last_feedback_count {
            // We received new feedback
            last_feedback_count = current_count;
            no_feedback_cycles = 0;

            // Check if we have all feedback (order + 1 values)
            let feedback = unsafe { LAST_FEEDBACK.as_ref() };
            if let Some(f) = feedback {
                if f.sequence.len() as i32 > goal.order {
                    info!("Received all feedback, action completed!");
                    info!("Final sequence: {:?}", f.sequence.as_slice());
                    break;
                }
            }
        } else {
            no_feedback_cycles += 1;
            // If no feedback for 5 seconds after goal accepted, something might be wrong
            if no_feedback_cycles > 50 && last_feedback_count == 0 {
                error!("No feedback received after 5 seconds");
                error!("Check that the action server is running");
                break;
            }
        }

        if cycle == max_wait_cycles - 1 {
            error!("Timeout waiting for action completion");
        }
    }

    info!("Action client finished");

    // Keep alive to allow cleanup
    loop {
        zephyr::time::sleep(zephyr::time::Duration::secs(10));
        info!("Action client idle (received {} feedback messages)", FEEDBACK_COUNT.load(Ordering::Relaxed));
    }
}
