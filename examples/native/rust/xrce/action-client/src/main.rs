//! XRCE-DDS action client — Fibonacci action via XRCE Agent.
//!
//! Composes the action protocol from 2 service clients (send_goal, get_result)
//! and 1 subscriber (feedback). Matches the wire format used by `nros-node`'s
//! `ConnectedActionClient`.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR     — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID      — ROS domain ID (default: 0)
//!   XRCE_FIBONACCI_ORDER — Fibonacci sequence order to request (default: 5)

use nros::xrce::*;
use nros::{
    CdrReader, CdrWriter, Deserialize, GoalId, GoalStatus, QosSettings, RosAction, Serialize,
    ServiceClientTrait, ServiceInfo, Session, Subscriber, TopicInfo, XrceSession,
};
use std::time::Instant;

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciResult};

fn main() {
    let agent_addr =
        std::env::var("XRCE_AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:2019".to_string());
    let domain_id: u32 = std::env::var("XRCE_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let order: i32 = std::env::var("XRCE_FIBONACCI_ORDER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    eprintln!(
        "XRCE Action Client: agent={}, domain={}, order={}",
        agent_addr, domain_id, order
    );

    init_posix_udp(&agent_addr);
    let mut executor =
        XrceExecutor::new("xrce_action_client", domain_id).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Build DDS type names for action sub-entities
    let action_type = Fibonacci::ACTION_NAME;
    let send_goal_type = format!("{}SendGoal_", action_type);
    let get_result_type = format!("{}GetResult_", action_type);
    let feedback_type = format!("{}FeedbackMessage_", action_type);

    // Create action sub-entities via raw session (action protocol is manual)
    let session: &mut XrceSession = executor.session_mut();

    let send_goal_info = ServiceInfo::new("/fibonacci/_action/send_goal", &send_goal_type, "");
    let mut send_goal_client = session
        .create_service_client(&send_goal_info)
        .expect("Failed to create send_goal client");
    eprintln!("send_goal service client created");

    let get_result_info = ServiceInfo::new("/fibonacci/_action/get_result", &get_result_type, "");
    let mut get_result_client = session
        .create_service_client(&get_result_info)
        .expect("Failed to create get_result client");
    eprintln!("get_result service client created");

    let feedback_topic = TopicInfo::new("/fibonacci/_action/feedback", &feedback_type, "");
    let mut feedback_subscriber = session
        .create_subscriber(&feedback_topic, QosSettings::BEST_EFFORT)
        .expect("Failed to create feedback subscriber");
    eprintln!("feedback subscriber created");

    println!("Action client ready");

    let mut req_buf = [0u8; 512];
    let mut reply_buf = [0u8; 512];
    let mut feedback_buf = [0u8; 512];

    // --- Step 1: Send goal ---
    // SendGoal request: GoalId(16 bytes) + FibonacciGoal(i32 order)
    let client_goal_id = GoalId::from_counter(1);
    {
        let mut writer = CdrWriter::new_with_header(&mut req_buf).unwrap();
        client_goal_id.serialize(&mut writer).unwrap();
        writer.write_i32(order).unwrap();
        let req_len = writer.position();

        let reply_len = send_goal_client
            .call_raw(&req_buf[..req_len], &mut reply_buf)
            .expect("send_goal call failed");

        // Parse response: bool(accepted) + i32(stamp_sec) + u32(stamp_nsec) + GoalId
        let mut reader = CdrReader::new_with_header(&reply_buf[..reply_len]).unwrap();
        let accepted = reader.read_bool().unwrap_or(false);
        let _stamp_sec = reader.read_i32().unwrap_or(0);
        let _stamp_nsec = reader.read_u32().unwrap_or(0);
        let server_goal_id = GoalId::deserialize(&mut reader).unwrap_or_default();

        if accepted {
            println!("Goal accepted: {:?}", server_goal_id);
        } else {
            println!("Goal rejected");
            let _ = executor.close();
            return;
        }
    }

    // --- Step 2: Wait for feedback ---
    let mut feedback_count = 0usize;
    let start = Instant::now();
    let feedback_timeout = std::time::Duration::from_secs(15);

    while start.elapsed() < feedback_timeout {
        executor.spin_once(100);

        // Check for feedback: GoalId(16 bytes) + FibonacciFeedback
        match feedback_subscriber.try_recv_raw(&mut feedback_buf) {
            Ok(Some(len)) => {
                if let Ok(mut reader) = CdrReader::new_with_header(&feedback_buf[..len]) {
                    let _fb_goal_id = GoalId::deserialize(&mut reader).unwrap_or_default();
                    if let Ok(feedback) = FibonacciFeedback::deserialize(&mut reader) {
                        feedback_count += 1;
                        println!(
                            "Feedback {}: sequence_len={}",
                            feedback_count,
                            feedback.sequence.len()
                        );

                        // Check if we have the final feedback (order + 1 elements)
                        if feedback.sequence.len() as i32 > order {
                            println!("All feedback received");
                            break;
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("Feedback receive error: {:?}", e);
            }
        }
    }

    // Small delay to let server finish storing result
    for _ in 0..5 {
        executor.spin_once(100);
    }

    // --- Step 3: Get result ---
    {
        let mut writer = CdrWriter::new_with_header(&mut req_buf).unwrap();
        // GetResult request: GoalId — use a zero ID to match "any completed goal"
        // (our server accepts zero ID as a wildcard)
        GoalId::zero().serialize(&mut writer).unwrap();
        let req_len = writer.position();

        let reply_len = get_result_client
            .call_raw(&req_buf[..req_len], &mut reply_buf)
            .expect("get_result call failed");

        // Parse response: i8(status) + FibonacciResult
        let mut reader = CdrReader::new_with_header(&reply_buf[..reply_len]).unwrap();
        let status_val = reader.read_i8().unwrap_or(0);
        let status = GoalStatus::from_i8(status_val).unwrap_or_default();

        if let Ok(result) = FibonacciResult::deserialize(&mut reader) {
            println!("Result: status={}, sequence={:?}", status, result.sequence);
        } else {
            println!("Result: status={}, (no result data)", status);
        }
    }

    println!("Action client done");
    let _ = executor.close();
}
