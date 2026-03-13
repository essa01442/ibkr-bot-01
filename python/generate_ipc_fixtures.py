import msgpack
import os

os.makedirs('fixtures/ipc', exist_ok=True)

def write_fixture(name, data):
    with open(f'fixtures/ipc/{name}.msgpack', 'wb') as f:
        f.write(msgpack.packb(data))

# Valid NewOrder
valid_new_order = {
    "NewOrder": {
        "symbol_id": 1234,
        "side": "Bid",
        "qty": 100,
        "order_type": "Limit",
        "limit_price": 45.20,
        "stop_price": None,
        "tif": "DAY",
        "idempotency_key": "order-1234-xyz",
        "take_profit_price": 46.00,
        "stop_loss_price": 44.50
    }
}
write_fixture('valid_new_order', valid_new_order)

# Invalid NewOrder (Missing symbol_id and side is int instead of str)
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

# Valid CancelOrder
valid_cancel_order = {
    "CancelOrder": {
        "order_id": 9876543210
    }
}
write_fixture('valid_cancel_order', valid_cancel_order)

# Invalid CancelOrder (Missing order_id)
invalid_cancel_order = {
    "CancelOrder": {}
}
write_fixture('invalid_cancel_order', invalid_cancel_order)

print("Generated MessagePack fixtures in fixtures/ipc/")
