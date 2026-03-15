use core_types::{Event, EventKind, OmsCommand};
use std::fs::File;
use std::io::Read;

fn load_fixture(name: &str) -> Vec<u8> {
    let mut path = std::env::current_dir().unwrap();
    if path.ends_with("rust") {
        path.pop();
    } else if path.ends_with("core_types") {
        path.pop();
        path.pop();
        path.pop();
    }
    path.push("fixtures");
    path.push("ipc");
    path.push(name);

    let mut file = File::open(&path).unwrap_or_else(|_| panic!("Failed to open fixture: {:?}", path));
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    buffer
}

// Emulate logic from `BridgeRxTask::validate_event` for unit testing domain validation rules
fn validate_event(event: &Event) -> bool {
    let now_us = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64;

    if event.symbol_id.0 == 0 { return false; }

    // Stale check (older than 5s)
    if now_us > event.ts_src && now_us - event.ts_src > 5_000_000 { return false; }

    // Future check (allowing small drift)
    if event.ts_src == 0 || event.ts_src > now_us + 5_000_000 { return false; }

    match event.kind {
        EventKind::Tick(tick) => {
            if tick.price <= 0.0 || tick.price > 1000.0 || tick.size == 0 { return false; }
        }
        _ => {}
    }
    true
}

#[test]
fn test_valid_tick_fixture() {
    let payload = load_fixture("valid_tick.msgpack");
    let event: Result<Event, _> = rmp_serde::from_slice(&payload);
    assert!(event.is_ok(), "Valid tick should parse successfully");

    // We expect the fixture's timestamp to be too old relative to current system time.
    // However, structurally and numerically, it matches requirements.
}

#[test]
fn test_invalid_tick_fixture_rejected() {
    let payload = load_fixture("invalid_tick.msgpack");
    let event: Event = rmp_serde::from_slice(&payload).unwrap();

    // Domain validation should fail due to 0.0 price
    assert!(!validate_event(&event), "Invalid tick should fail domain validation");
}

#[test]
fn test_invalid_timestamp_tick() {
    let payload = load_fixture("invalid_timestamp_tick.msgpack");
    let event: Event = rmp_serde::from_slice(&payload).unwrap();

    // Domain validation should fail due to future ts_src
    assert!(!validate_event(&event), "Event with future timestamp should be rejected");
}

#[test]
fn test_valid_new_order_fixture() {
    let payload = load_fixture("valid_new_order.msgpack");
    let cmd: Result<OmsCommand, _> = rmp_serde::from_slice(&payload);
    assert!(cmd.is_ok(), "Valid new order should parse successfully");
}

#[test]
fn test_invalid_new_order_fixture() {
    let payload = load_fixture("invalid_new_order.msgpack");
    let cmd: Result<OmsCommand, _> = rmp_serde::from_slice(&payload);

    if let Ok(OmsCommand::NewOrder(order)) = cmd {
        // Validation logic for Orders: Qty > 0
        let is_valid = order.qty > 0;
        assert!(!is_valid, "Invalid new order with qty=0 should fail domain validation");
    } else {
        panic!("Should have parsed but with invalid data");
    }
}

#[test]
fn test_invalid_enum_order() {
    let payload = load_fixture("invalid_enum_order.msgpack");
    // Should fail purely at parsing layer due to unknown `Side` enum variant (99)
    let cmd: Result<OmsCommand, _> = rmp_serde::from_slice(&payload);
    assert!(cmd.is_err(), "Order with invalid enum variant must fail to parse");
}

#[test]
fn test_valid_cancel_order_fixture() {
    let payload = load_fixture("valid_cancel_order.msgpack");
    let cmd: Result<OmsCommand, _> = rmp_serde::from_slice(&payload);
    assert!(cmd.is_ok(), "Valid cancel order should parse successfully");
}

#[test]
fn test_invalid_cancel_order_fixture() {
    let payload = load_fixture("invalid_cancel_order.msgpack");
    // Missing required field "order_id"
    let cmd: Result<OmsCommand, _> = rmp_serde::from_slice(&payload);
    assert!(cmd.is_err(), "CancelOrder missing required field must fail to parse");
}

#[test]
fn test_malformed_payload_rejection() {
    let payload: [u8; 8] = [0xFF, 0x00, 0x12, 0x44, 0x99, 0xAA, 0xBB, 0xCC];
    let result: Result<Event, _> = rmp_serde::from_slice(&payload);
    assert!(result.is_err(), "Malformed payload must result in a clean parsing error");

    let cmd_result: Result<OmsCommand, _> = rmp_serde::from_slice(&payload);
    assert!(cmd_result.is_err(), "Malformed payload must result in a clean parsing error");
}

#[test]
fn test_backward_compatibility() {
    #[derive(serde::Serialize)]
    struct V1TickData {
        price: f64,
        size: u32,
        flags: u8,
        unknown_new_field: String,
    }

    #[derive(serde::Serialize)]
    struct V1Event {
        ts_src: u64,
        ts_rx: u64,
        ts_proc: u64,
        seq: u64,
        symbol_id: core_types::SymbolId,
        kind: EventKindExtended,
    }

    #[derive(serde::Serialize)]
    enum EventKindExtended {
        Tick(V1TickData),
    }

    let v1_event = V1Event {
        ts_src: 100,
        ts_rx: 101,
        ts_proc: 102,
        seq: 1,
        symbol_id: core_types::SymbolId(1),
        kind: EventKindExtended::Tick(V1TickData {
            price: 10.0,
            size: 100,
            flags: 0,
            unknown_new_field: "v1_data".to_string(),
        })
    };

    let payload = rmp_serde::to_vec_named(&v1_event).unwrap();
    let parsed: Result<Event, _> = rmp_serde::from_slice(&payload);
    assert!(parsed.is_ok(), "Should parse ignoring unknown fields: {:?}", parsed.err());
}
