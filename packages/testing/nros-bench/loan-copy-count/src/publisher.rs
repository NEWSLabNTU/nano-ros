//! phase-510 W5 — the measured image. Publishes [`PER_PATH`] samples through
//! `publish_raw`, then [`PER_PATH`] through `try_loan` + `commit`, counting the
//! payload-sized copies each publish call makes, and prints
//!
//! ```text
//! LOAN-COPY raw_publishes=20 raw_copies=<n> loan_publishes=20 loan_copies=<m>
//! ```
//!
//! before exiting through semihosting.

#![no_std]
#![no_main]

use loan_copy_count::{
    MARK_LOAN, MARK_RAW, PAYLOAD_LEN, PER_PATH, TOPIC, TYPE_HASH, TYPE_NAME, count_payload_copies,
    publisher_config,
};
use loan_copy_count_nros_selection::nros_board_mps2_an385_freertos::{Mps2An385, println};
use nros::prelude::*;
use nros_node::executor::EmbeddedRawPublisher;

fn fill(buf: &mut [u8], mark: u8, seq: u32) {
    buf[0] = mark;
    buf[1..5].copy_from_slice(&seq.to_le_bytes());
    for (i, b) in buf[5..].iter_mut().enumerate() {
        *b = (i as u8).wrapping_add(seq as u8);
    }
}

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    let _ = Mps2An385::run_bare(publisher_config(), |config| {
        let exec_config = ExecutorConfig::new(config.base.zenoh_locator)
            .domain_id(config.base.domain_id)
            .node_name("loan-copy-pub");
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config)?;
        let nid = executor.node_builder("loan-copy-pub").build()?;
        let publisher: EmbeddedRawPublisher =
            executor.with_node_try(nid, |n| n.create_publisher_raw(TOPIC, TYPE_NAME, TYPE_HASH))?;

        // Let the router and the subscriber image's session settle.
        for _ in 0..300 {
            executor.spin_once(core::time::Duration::from_millis(10));
        }
        println!("loan-copy-pub ready — publishing on {}", TOPIC);

        let mut raw_copies = 0u32;
        let mut buf = [0u8; PAYLOAD_LEN];
        for seq in 0..PER_PATH {
            fill(&mut buf, MARK_RAW, seq);
            let (r, n) = count_payload_copies(|| publisher.publish_serialized(&buf));
            if r.is_err() {
                println!("LOAN-COPY-ERROR publish_raw seq={}", seq);
            }
            raw_copies += n;
            for _ in 0..10 {
                executor.spin_once(core::time::Duration::from_millis(10));
            }
        }

        let mut loan_copies = 0u32;
        let mut loaned = 0u32;
        for seq in 0..PER_PATH {
            // The caller's own fill of the slot is not a copy by the stack:
            // `fill` writes byte by byte, never a `PAYLOAD_LEN` memcpy.
            let (r, n) = count_payload_copies(|| match publisher.try_loan(PAYLOAD_LEN) {
                Ok(mut loan) => {
                    fill(loan.as_mut(), MARK_LOAN, seq);
                    loan.commit().map(|_| true)
                }
                Err(_) => Ok(false),
            });
            match r {
                Ok(true) => loaned += 1,
                Ok(false) => println!("LOAN-COPY-ERROR try_loan refused seq={}", seq),
                Err(_) => println!("LOAN-COPY-ERROR commit seq={}", seq),
            }
            loan_copies += n;
            for _ in 0..10 {
                executor.spin_once(core::time::Duration::from_millis(10));
            }
        }

        println!(
            "LOAN-COPY raw_publishes={} raw_copies={} loan_publishes={} loan_copies={}",
            PER_PATH, raw_copies, loaned, loan_copies
        );
        // Keep the session up while the last samples drain to the subscriber.
        for _ in 0..200 {
            executor.spin_once(core::time::Duration::from_millis(10));
        }
        cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    });
    loop {}
}
