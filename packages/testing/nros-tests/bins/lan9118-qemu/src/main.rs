//! QEMU test for LAN9118 Ethernet driver on MPS2-AN385
//!
//! This test verifies that the LAN9118 driver correctly initializes
//! the Ethernet controller in QEMU.
//!
//! Run with: `cargo run --release` (in the example directory)

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use panic_semihosting as _;

use lan9118_smoltcp::{Config, Lan9118, MPS2_AN385_BASE};

/// Test device detection
fn test_device_detect() -> bool {
    let config = Config {
        base_addr: MPS2_AN385_BASE,
        mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
    };

    match unsafe { Lan9118::new(config) } {
        Ok(_) => true,
        Err(e) => {
            hprintln!("  Error creating driver: {:?}", e);
            false
        }
    }
}

/// Test driver initialization
fn test_init() -> bool {
    let config = Config::default();

    let mut eth = match unsafe { Lan9118::new(config) } {
        Ok(e) => e,
        Err(e) => {
            hprintln!("  Error creating driver: {:?}", e);
            return false;
        }
    };

    match eth.init() {
        Ok(()) => true,
        Err(e) => {
            hprintln!("  Error initializing: {:?}", e);
            false
        }
    }
}

/// Test MAC address configuration
fn test_mac_address() -> bool {
    let test_mac = [0x02, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
    let config = Config {
        base_addr: MPS2_AN385_BASE,
        mac_addr: test_mac,
    };

    let mut eth = match unsafe { Lan9118::new(config) } {
        Ok(e) => e,
        Err(_) => return false,
    };

    if eth.init().is_err() {
        return false;
    }

    // Verify MAC address was stored
    let mac = eth.mac_address();
    mac == test_mac
}

/// Test link status check (will likely be down in QEMU without network)
fn test_link_status() -> bool {
    let config = Config::default();

    let mut eth = match unsafe { Lan9118::new(config) } {
        Ok(e) => e,
        Err(_) => return false,
    };

    if eth.init().is_err() {
        return false;
    }

    // Just verify the function runs without crashing
    let _link_up = eth.link_is_up();
    true
}

/// Test smoltcp Device trait
fn test_device_trait() -> bool {
    use smoltcp::phy::Device;

    let config = Config::default();

    let mut eth = match unsafe { Lan9118::new(config) } {
        Ok(e) => e,
        Err(_) => return false,
    };

    if eth.init().is_err() {
        return false;
    }

    // Get capabilities
    let caps = eth.capabilities();
    if caps.max_transmission_unit != 1500 {
        hprintln!("  Bad MTU: {}", caps.max_transmission_unit);
        return false;
    }

    // Try to transmit (will succeed even without network)
    let timestamp = smoltcp::time::Instant::from_millis(0);
    if let Some(tx_token) = eth.transmit(timestamp) {
        use smoltcp::phy::TxToken;
        // Create a minimal Ethernet frame
        tx_token.consume(64, |buf| {
            // Fill with dummy data
            for (i, b) in buf.iter_mut().enumerate() {
                *b = i as u8;
            }
        });
    }

    true
}

#[entry]
fn main() -> ! {
    hprintln!("");
    hprintln!("========================================");
    hprintln!("  LAN9118 Driver Test (MPS2-AN385)");
    hprintln!("========================================");
    hprintln!("");

    let mut passed = 0;
    let mut failed = 0;

    macro_rules! run_test {
        ($name:expr, $test:expr) => {
            if $test {
                hprintln!("[PASS] {}", $name);
                passed += 1;
            } else {
                hprintln!("[FAIL] {}", $name);
                failed += 1;
            }
        };
    }

    hprintln!("--- Driver Tests ---");
    run_test!("Device detection", test_device_detect());
    run_test!("Driver init", test_init());
    run_test!("MAC address", test_mac_address());
    run_test!("Link status check", test_link_status());
    run_test!("smoltcp Device trait", test_device_trait());

    hprintln!("");
    hprintln!("----------------------------------------");
    hprintln!("  Results: {} passed, {} failed", passed, failed);
    hprintln!("----------------------------------------");
    hprintln!("");

    if failed == 0 {
        hprintln!("All tests passed!");
        cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);
    } else {
        hprintln!("Some tests failed!");
        cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_FAILURE);
    }

    loop {
        cortex_m::asm::wfi();
    }
}
