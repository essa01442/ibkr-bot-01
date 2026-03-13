use core_types::{Event, EventKind, TickData, SymbolId, OmsCommand, OrderRequest, Side, OrderType, TimeInForce, CancelRequest};
use std::fs::File;
use std::io::Write;

fn main() {
    println!("Generating fixtures...");

    // Create Valid Tick Event
    let valid_tick = Event {
        ts_src: 1672531200000000,
        ts_rx: 1672531200001000,
        ts_proc: 1672531200002000,
        seq: 1,
        symbol_id: SymbolId(42),
        kind: EventKind::Tick(TickData {
            price: 15.50,
            size: 100,
            flags: 0,
        }),
    };

    // Invalid Tick: 0 price
    let invalid_tick = Event {
        ts_src: 1672531200000000,
        ts_rx: 1672531200001000,
        ts_proc: 1672531200002000,
        seq: 2,
        symbol_id: SymbolId(42),
        kind: EventKind::Tick(TickData {
            price: 0.0,
            size: 100,
            flags: 0,
        }),
    };

    // Invalid Timestamp Tick: Future ts_src
    let future_tick = Event {
        ts_src: 253402300799000000, // Year 9999
        ts_rx: 1672531200001000,
        ts_proc: 1672531200002000,
        seq: 3,
        symbol_id: SymbolId(42),
        kind: EventKind::Tick(TickData {
            price: 15.50,
            size: 100,
            flags: 0,
        }),
    };

    // Valid NewOrder Command
    let valid_order = OmsCommand::NewOrder(OrderRequest {
        symbol_id: SymbolId(42),
        side: Side::Bid,
        qty: 100,
        order_type: OrderType::Limit,
        limit_price: Some(15.50),
        stop_price: None,
        tif: TimeInForce::Day,
        idempotency_key: arrayvec::ArrayString::from("order-123").unwrap(),
        take_profit_price: None,
        stop_loss_price: None,
    });

    // Invalid new order: size 0
    let invalid_order = OmsCommand::NewOrder(OrderRequest {
        symbol_id: SymbolId(42),
        side: Side::Bid,
        qty: 0,
        order_type: OrderType::Limit,
        limit_price: Some(15.50),
        stop_price: None,
        tif: TimeInForce::Day,
        idempotency_key: arrayvec::ArrayString::from("order-123").unwrap(),
        take_profit_price: None,
        stop_loss_price: None,
    });

    // Valid Cancel Order Command
    let valid_cancel = OmsCommand::CancelOrder(CancelRequest {
        order_id: 999,
    });

    // We can't easily construct a struct missing a required field using Rust struct,
    // so we will construct a HashMap and serialize it to simulate missing fields and invalid enums.

    use serde::Serialize;
    use std::collections::HashMap;

    let mut invalid_cancel = HashMap::new();
    let mut cancel_content = HashMap::new();
    // Missing order_id completely
    cancel_content.insert("some_other_field", 123);
    invalid_cancel.insert("CancelOrder", cancel_content);

    // Invalid enum: side = 99
    #[derive(Serialize)]
    struct InvalidEnumOrderRequest {
        pub symbol_id: SymbolId,
        pub side: u8, // Using u8 instead of Side to force invalid value
        pub qty: u32,
        pub order_type: OrderType,
        pub limit_price: Option<f64>,
        pub stop_price: Option<f64>,
        pub tif: TimeInForce,
        pub idempotency_key: String,
        pub take_profit_price: Option<f64>,
        pub stop_loss_price: Option<f64>,
    }

    #[derive(Serialize)]
    enum InvalidEnumOmsCommand {
        NewOrder(InvalidEnumOrderRequest),
    }

    let invalid_enum_order = InvalidEnumOmsCommand::NewOrder(InvalidEnumOrderRequest {
        symbol_id: SymbolId(42),
        side: 99, // Invalid
        qty: 100,
        order_type: OrderType::Limit,
        limit_price: Some(15.50),
        stop_price: None,
        tif: TimeInForce::Day,
        idempotency_key: "order-123".to_string(),
        take_profit_price: None,
        stop_loss_price: None,
    });

    let mut dir = std::env::current_dir().unwrap();
    if dir.ends_with("rust") {
        dir.pop();
    }
    let fixtures_dir = dir.join("fixtures").join("ipc");
    std::fs::create_dir_all(&fixtures_dir).unwrap();

    File::create(fixtures_dir.join("valid_tick.msgpack")).unwrap().write_all(&rmp_serde::to_vec_named(&valid_tick).unwrap()).unwrap();
    File::create(fixtures_dir.join("invalid_tick.msgpack")).unwrap().write_all(&rmp_serde::to_vec_named(&invalid_tick).unwrap()).unwrap();
    File::create(fixtures_dir.join("invalid_timestamp_tick.msgpack")).unwrap().write_all(&rmp_serde::to_vec_named(&future_tick).unwrap()).unwrap();

    File::create(fixtures_dir.join("valid_new_order.msgpack")).unwrap().write_all(&rmp_serde::to_vec_named(&valid_order).unwrap()).unwrap();
    File::create(fixtures_dir.join("invalid_new_order.msgpack")).unwrap().write_all(&rmp_serde::to_vec_named(&invalid_order).unwrap()).unwrap();
    File::create(fixtures_dir.join("invalid_enum_order.msgpack")).unwrap().write_all(&rmp_serde::to_vec_named(&invalid_enum_order).unwrap()).unwrap();

    File::create(fixtures_dir.join("valid_cancel_order.msgpack")).unwrap().write_all(&rmp_serde::to_vec_named(&valid_cancel).unwrap()).unwrap();
    File::create(fixtures_dir.join("invalid_cancel_order.msgpack")).unwrap().write_all(&rmp_serde::to_vec_named(&invalid_cancel).unwrap()).unwrap();

    println!("Fixtures generated successfully.");
}
