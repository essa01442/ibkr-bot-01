use super::*;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn get_fixture_path(name: &str) -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    PathBuf::from(manifest_dir)
        .parent().unwrap()
        .parent().unwrap()
        .parent().unwrap()
        .join("fixtures/ipc")
        .join(name)
}

#[test]
fn test_valid_new_order_fixture() {
    let data = fs::read(get_fixture_path("valid_new_order.msgpack")).unwrap();
    let command: OmsCommand = rmp_serde::from_slice(&data).expect("Must parse valid NewOrder");

    match command {
        OmsCommand::NewOrder(req) => {
            assert_eq!(req.symbol_id.0, 1234);
            assert_eq!(req.qty, 100);
            assert_eq!(req.side, Side::Bid);
            assert_eq!(req.order_type, OrderType::Limit);
            assert_eq!(req.limit_price, Some(45.20));
            assert_eq!(req.tif, TimeInForce::GTC);
            assert_eq!(req.idempotency_key.as_str(), "order-1234-xyz");
        }
        _ => panic!("Expected NewOrder"),
    }
}

#[test]
fn test_invalid_new_order_fixture() {
    let data = fs::read(get_fixture_path("invalid_new_order.msgpack")).unwrap();
    let result: Result<OmsCommand, _> = rmp_serde::from_slice(&data);
    // Malformed payload rejection: invalid bytes return clean error, no panic
    assert!(result.is_err(), "Invalid NewOrder must fail parsing gracefully");
}

#[test]
fn test_valid_cancel_order_fixture() {
    let data = fs::read(get_fixture_path("valid_cancel_order.msgpack")).unwrap();
    let command: OmsCommand = rmp_serde::from_slice(&data).expect("Must parse valid CancelOrder");

    match command {
        OmsCommand::CancelOrder(req) => {
            assert_eq!(req.order_id, 9876543210);
        }
        _ => panic!("Expected CancelOrder"),
    }
}

#[test]
fn test_invalid_cancel_order_fixture() {
    let data = fs::read(get_fixture_path("invalid_cancel_order.msgpack")).unwrap();
    let result: Result<OmsCommand, _> = rmp_serde::from_slice(&data);
    assert!(result.is_err(), "Invalid CancelOrder must fail gracefully");
}

#[test]
fn test_valid_event_fixture() {
    let data = fs::read(get_fixture_path("valid_event.msgpack")).unwrap();
    let event: Event = rmp_serde::from_slice(&data).expect("Must parse valid Event");

    // Numeric domain check
    assert!(event.symbol_id.0 > 0, "Symbol ID must be non-empty/positive");

    // Timestamp validity check
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
    assert!(event.ts_src <= now + 1_000_000, "Timestamp must not be in the far future");
    assert!(event.ts_src > 0, "Timestamp must be positive");

    match event.kind {
        EventKind::Tick(tick) => {
            assert!(tick.price > 0.0);
            assert!(tick.size > 0);
        }
        _ => panic!("Expected Tick"),
    }
}

#[test]
fn test_invalid_event_fixture_invariants() {
    let data = fs::read(get_fixture_path("invalid_event.msgpack")).unwrap();
    let event: Result<Event, _> = rmp_serde::from_slice(&data);

    if let Ok(ev) = event {
        // Even if it deserializes structurally, domain constraints must be verifiable
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64;
        let mut invariant_failed = false;

        if ev.ts_src > now + 3600_000_000 { // Way in future
            invariant_failed = true;
        }
        if ev.symbol_id.0 == 0 {
            invariant_failed = true;
        }

        match ev.kind {
            EventKind::Tick(t) => {
                if t.price <= 0.0 || t.size == 0 {
                    invariant_failed = true;
                }
            }
            _ => {}
        }

        assert!(invariant_failed, "Invalid event passed invariant checks but should have failed");
    }
}

#[test]
fn test_enum_stability() {
    // Enum stability: enum values cannot change meaning between versions.
    // Ensure serialization aligns exactly with numeric representations if tagged
    let side_bid_json = serde_json::to_string(&Side::Bid).unwrap();
    assert_eq!(side_bid_json, "\"Bid\""); // Serde uses string by default, providing backward compat stability.
}
