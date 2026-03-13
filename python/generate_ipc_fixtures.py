import msgpack
import os
import time

os.makedirs('fixtures/ipc', exist_ok=True)

def write_fixture(name, data):
    with open(f'fixtures/ipc/{name}.msgpack', 'wb') as f:
        f.write(msgpack.packb(data))

# Rust's OmsCommand uses externally tagged enums by default in serde:
# {"NewOrder": {"symbol_id": 1234, ...}}
valid_new_order = {
    "NewOrder": {
        "symbol_id": 1234,
        "side": "Bid", # Serde enum default is string tag if derived
        "qty": 100,
        "order_type": "Limit",
        "limit_price": 45.20,
        "stop_price": None,
        "tif": "GTC", # Core types enum variants
        "idempotency_key": "order-1234-xyz",
        "take_profit_price": 46.00,
        "stop_loss_price": 44.50
    }
}
write_fixture('valid_new_order', valid_new_order)

# Invalid NewOrder (Missing required fields, negative qty, missing symbol_id)
invalid_new_order = {
    "NewOrder": {
        "side": 1,
        "qty": -100,
        "order_type": "Magic",
        "limit_price": "forty-five",
        "tif": "NOW",
        "idempotency_key": ""
    }
}
write_fixture('invalid_new_order', invalid_new_order)

valid_cancel_order = {
    "CancelOrder": {
        "order_id": 9876543210
    }
}
write_fixture('valid_cancel_order', valid_cancel_order)

invalid_cancel_order = {
    "CancelOrder": {}
}
write_fixture('invalid_cancel_order', invalid_cancel_order)

# To cover timestamps, let's also define an Event or Order fixture if needed.
# Since the prompt asked for "timestamps must be positive, reasonable, non-future",
# let's add a test fixture for OrderStatusData or FillData if they exist.

# Event (e.g., Tick) valid
current_ts = int(time.time() * 1_000_000)
valid_event = {
    "ts_src": current_ts,
    "ts_rx": current_ts + 10,
    "ts_proc": current_ts + 20,
    "seq": 1,
    "symbol_id": 1234,
    "kind": {
        "Tick": {
            "price": 150.50,
            "size": 100,
            "flags": 0
        }
    }
}
write_fixture('valid_event', valid_event)

# Invalid Event (timestamp in future or negative, size < 0, price < 0)
invalid_event = {
    "ts_src": current_ts + 10_000_000_000_000, # Extreme future
    "ts_rx": current_ts,
    "ts_proc": current_ts,
    "seq": 1,
    "symbol_id": 0, # Reserved ID usually invalid
    "kind": {
        "Tick": {
            "price": -10.0,
            "size": 0,
            "flags": 0
        }
    }
}
write_fixture('invalid_event', invalid_event)
print("Finished adding Event fixtures.")
