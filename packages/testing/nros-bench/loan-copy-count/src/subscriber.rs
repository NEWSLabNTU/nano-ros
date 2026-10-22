//! phase-510 W5 — the peer image: a plain raw subscription on [`TOPIC`] that
//! counts what arrives from each publish path (first payload byte) and checks
//! every sample is whole, then prints
//!
//! ```text
//! LOAN-COPY-RX raw=<n> loan=<m> short=<k>
//! ```
//!
//! once the loan samples stop arriving, and exits.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};

use loan_copy_count::{
    MARK_LOAN, MARK_RAW, PAYLOAD_LEN, PER_PATH, TOPIC, TYPE_HASH, TYPE_NAME, subscriber_config,
};
use loan_copy_count_nros_selection::nros_board_mps2_an385_freertos::{Mps2An385, println};
use nros::prelude::*;

static RX_RAW: AtomicU32 = AtomicU32::new(0);
static RX_LOAN: AtomicU32 = AtomicU32::new(0);
static RX_SHORT: AtomicU32 = AtomicU32::new(0);

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    let _ = Mps2An385::run_bare(subscriber_config(), |config| {
        let exec_config = ExecutorConfig::new(config.base.zenoh_locator)
            .domain_id(config.base.domain_id)
            .node_name("loan-copy-sub");
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config)?;
        let nid = executor.node_builder("loan-copy-sub").build()?;
        executor
            .node_mut(nid)
            .subscription(TOPIC)
            .generic(TYPE_NAME, TYPE_HASH)
            .qos(Default::default())
            .rx_buffer::<512>()
            .build(|bytes: &[u8]| {
                if bytes.len() != PAYLOAD_LEN {
                    RX_SHORT.fetch_add(1, Ordering::Relaxed);
                } else if bytes[0] == MARK_RAW {
                    RX_RAW.fetch_add(1, Ordering::Relaxed);
                } else if bytes[0] == MARK_LOAN {
                    RX_LOAN.fetch_add(1, Ordering::Relaxed);
                }
            })?;
        println!("loan-copy-sub subscribed on {}", TOPIC);

        // Up to ~60 s; done once every loan sample is in, or the loan stream
        // has gone quiet for 5 s after starting.
        let mut quiet = 0u32;
        let mut last = 0u32;
        for _ in 0..600 {
            executor.spin_once(core::time::Duration::from_millis(100));
            let loan = RX_LOAN.load(Ordering::Relaxed);
            if loan >= PER_PATH {
                break;
            }
            if loan > 0 && loan == last {
                quiet += 1;
                if quiet >= 50 {
                    break;
                }
            } else {
                quiet = 0;
            }
            last = loan;
        }
        println!(
            "LOAN-COPY-RX raw={} loan={} short={}",
            RX_RAW.load(Ordering::Relaxed),
            RX_LOAN.load(Ordering::Relaxed),
            RX_SHORT.load(Ordering::Relaxed)
        );
        cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    });
    loop {}
}
