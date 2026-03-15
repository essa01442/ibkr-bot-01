use core_types::{
    CancelAckData, CancelRejectData, CancelRequest, Event, EventKind, FillData, L2DeltaData,
    OmsCommand, OrderRequest, OrderStatus, OrderStatusData, OrderType, RejectData, Side,
    SnapshotData, SymbolId, TickData, TimeInForce,
};
use std::fs::File;
use std::io::Write;

fn main() {
    println!("Generating fixtures...");

    let mut dir = std::env::current_dir().unwrap();
    if dir.ends_with("rust") {
        dir.pop();
    }
    let fixtures_dir = dir.join("fixtures").join("ipc");
    std::fs::create_dir_all(&fixtures_dir).unwrap();

    let write_fix = |name: &str, data: &Vec<u8>| {
        File::create(fixtures_dir.join(name))
            .unwrap()
            .write_all(data)
            .unwrap();
    };

    // Valid Tick
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
    write_fix(
        "valid_tick.msgpack",
        &rmp_serde::to_vec_named(&valid_tick).unwrap(),
    );

    // Invalid Tick
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
    write_fix(
        "invalid_tick.msgpack",
        &rmp_serde::to_vec_named(&invalid_tick).unwrap(),
    );

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
    write_fix(
        "invalid_timestamp_tick.msgpack",
        &rmp_serde::to_vec_named(&future_tick).unwrap(),
    );

    // L2Delta
    let valid_l2 = Event {
        ts_src: 1,
        ts_rx: 1,
        ts_proc: 1,
        seq: 4,
        symbol_id: SymbolId(42),
        kind: EventKind::L2Delta(L2DeltaData {
            price: 15.50,
            size: 100,
            side: Side::Bid,
            level: 0,
            is_delete: false,
        }),
    };
    write_fix(
        "valid_l2delta.msgpack",
        &rmp_serde::to_vec_named(&valid_l2).unwrap(),
    );

    let invalid_l2 = Event {
        ts_src: 1,
        ts_rx: 1,
        ts_proc: 1,
        seq: 5,
        symbol_id: SymbolId(42),
        kind: EventKind::L2Delta(L2DeltaData {
            price: 0.0,
            size: 100,
            side: Side::Bid,
            level: 0,
            is_delete: false,
        }),
    };
    write_fix(
        "invalid_l2delta.msgpack",
        &rmp_serde::to_vec_named(&invalid_l2).unwrap(),
    );

    // Snapshot
    let valid_snap = Event {
        ts_src: 1,
        ts_rx: 1,
        ts_proc: 1,
        seq: 6,
        symbol_id: SymbolId(42),
        kind: EventKind::Snapshot(SnapshotData {
            bid_price: 10.0,
            ask_price: 11.0,
            bid_size: 1,
            ask_size: 1,
            volume: 100,
            avg_volume_20d: 100,
            has_news_today: false,
            weekly_ema: 10.0,
            daily_resistance: 12.0,
        }),
    };
    write_fix(
        "valid_snapshot.msgpack",
        &rmp_serde::to_vec_named(&valid_snap).unwrap(),
    );

    let invalid_snap = Event {
        ts_src: 1,
        ts_rx: 1,
        ts_proc: 1,
        seq: 7,
        symbol_id: SymbolId(42),
        kind: EventKind::Snapshot(SnapshotData {
            bid_price: 12.0,
            ask_price: 11.0,
            bid_size: 1,
            ask_size: 1,
            volume: 100,
            avg_volume_20d: 100,
            has_news_today: false,
            weekly_ema: 10.0,
            daily_resistance: 12.0,
        }),
    };
    write_fix(
        "invalid_snapshot.msgpack",
        &rmp_serde::to_vec_named(&invalid_snap).unwrap(),
    );

    // Fill
    let valid_fill = Event {
        ts_src: 1,
        ts_rx: 1,
        ts_proc: 1,
        seq: 8,
        symbol_id: SymbolId(42),
        kind: EventKind::Fill(FillData {
            order_id: 1,
            price: 10.0,
            size: 100,
            side: Side::Bid,
            liquidity: 0,
        }),
    };
    write_fix(
        "valid_fill.msgpack",
        &rmp_serde::to_vec_named(&valid_fill).unwrap(),
    );

    let invalid_fill = Event {
        ts_src: 1,
        ts_rx: 1,
        ts_proc: 1,
        seq: 9,
        symbol_id: SymbolId(42),
        kind: EventKind::Fill(FillData {
            order_id: 1,
            price: -10.0,
            size: 100,
            side: Side::Bid,
            liquidity: 0,
        }),
    };
    write_fix(
        "invalid_fill.msgpack",
        &rmp_serde::to_vec_named(&invalid_fill).unwrap(),
    );

    // OrderStatus
    let valid_os = Event {
        ts_src: 1,
        ts_rx: 1,
        ts_proc: 1,
        seq: 10,
        symbol_id: SymbolId(42),
        kind: EventKind::OrderStatus(OrderStatusData {
            order_id: 1,
            status: OrderStatus::Filled,
            filled_qty: 100,
            remaining_qty: 0,
            avg_fill_price: 10.0,
        }),
    };
    write_fix(
        "valid_order_status.msgpack",
        &rmp_serde::to_vec_named(&valid_os).unwrap(),
    );

    // Reject
    let valid_rej = Event {
        ts_src: 1,
        ts_rx: 1,
        ts_proc: 1,
        seq: 11,
        symbol_id: SymbolId(42),
        kind: EventKind::Reject(RejectData {
            order_id: 1,
            reason: "Test".into(),
            code: 0,
        }),
    };
    write_fix(
        "valid_reject.msgpack",
        &rmp_serde::to_vec_named(&valid_rej).unwrap(),
    );

    // CancelAck / Reject
    let valid_c_ack = Event {
        ts_src: 1,
        ts_rx: 1,
        ts_proc: 1,
        seq: 12,
        symbol_id: SymbolId(42),
        kind: EventKind::CancelAck(CancelAckData { order_id: 1 }),
    };
    write_fix(
        "valid_cancel_ack.msgpack",
        &rmp_serde::to_vec_named(&valid_c_ack).unwrap(),
    );

    let valid_c_rej = Event {
        ts_src: 1,
        ts_rx: 1,
        ts_proc: 1,
        seq: 13,
        symbol_id: SymbolId(42),
        kind: EventKind::CancelReject(CancelRejectData {
            order_id: 1,
            reason: "Test".into(),
        }),
    };
    write_fix(
        "valid_cancel_reject.msgpack",
        &rmp_serde::to_vec_named(&valid_c_rej).unwrap(),
    );

    // Heartbeat
    let valid_hb = Event {
        ts_src: 1,
        ts_rx: 1,
        ts_proc: 1,
        seq: 14,
        symbol_id: SymbolId(42),
        kind: EventKind::Heartbeat,
    };
    write_fix(
        "valid_heartbeat.msgpack",
        &rmp_serde::to_vec_named(&valid_hb).unwrap(),
    );

    // Commands
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
    write_fix(
        "valid_new_order.msgpack",
        &rmp_serde::to_vec_named(&valid_order).unwrap(),
    );

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
    write_fix(
        "invalid_new_order.msgpack",
        &rmp_serde::to_vec_named(&invalid_order).unwrap(),
    );

    let valid_cancel = OmsCommand::CancelOrder(CancelRequest { order_id: 999 });
    write_fix(
        "valid_cancel_order.msgpack",
        &rmp_serde::to_vec_named(&valid_cancel).unwrap(),
    );

    // Manual dictionary for malformed payloads
    use serde::Serialize;
    use std::collections::HashMap;

    let mut invalid_cancel = HashMap::new();
    let mut cancel_content = HashMap::new();
    cancel_content.insert("some_other_field", 123);
    invalid_cancel.insert("CancelOrder", cancel_content);
    write_fix(
        "invalid_cancel_order.msgpack",
        &rmp_serde::to_vec_named(&invalid_cancel).unwrap(),
    );

    #[derive(Serialize)]
    struct InvalidEnumOrderRequest {
        pub symbol_id: SymbolId,
        pub side: u8,
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
        side: 99,
        qty: 100,
        order_type: OrderType::Limit,
        limit_price: Some(15.50),
        stop_price: None,
        tif: TimeInForce::Day,
        idempotency_key: "order-123".to_string(),
        take_profit_price: None,
        stop_loss_price: None,
    });
    write_fix(
        "invalid_enum_order.msgpack",
        &rmp_serde::to_vec_named(&invalid_enum_order).unwrap(),
    );

    println!("Fixtures generated successfully.");
}
