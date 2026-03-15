import asyncio
import os
import sys
import time
import msgpack
import socket

def send_to_uds(path, data):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.connect(path)
        client.sendall(data)

async def run_tests(sock_path):
    print("Starting integration tests on", sock_path)

    # Scenario 1: Valid Message
    tick_event = {
        "ts_src": int(time.time() * 1_000_000),
        "ts_rx": int(time.time() * 1_000_000),
        "ts_proc": int(time.time() * 1_000_000),
        "seq": 1,
        "symbol_id": 42,
        "kind": {
            "Tick": {
                "price": 10.5,
                "size": 100,
                "flags": 0
            }
        }
    }
    encoded = msgpack.packb(tick_event)
    send_to_uds(sock_path, encoded)
    print("Sent valid tick.")

    # Scenario 4: Malformed payload
    send_to_uds(sock_path, b'\xff\x00\x11\x22')
    print("Sent malformed payload.")

    # Scenario 5 & 6: Heartbeat missing, then reconnect
    # We will send a heartbeat, wait > 1s, send another.
    heartbeat_event = {
        "ts_src": int(time.time() * 1_000_000),
        "ts_rx": int(time.time() * 1_000_000),
        "ts_proc": int(time.time() * 1_000_000),
        "seq": 2,
        "symbol_id": 42,
        "kind": "Heartbeat"
    }
    encoded_hb = msgpack.packb(heartbeat_event)
    send_to_uds(sock_path, encoded_hb)
    print("Sent first heartbeat.")

    # Sleep less than overall timeout of test but > 1.0s to trigger Degraded
    time.sleep(1.2) # Force degraded mode
    print("Waited > 1s for missing heartbeat (Scenario 5).")

    send_to_uds(sock_path, encoded_hb)
    print("Sent second heartbeat, recovering mode (Scenario 6).")

    print("All Python-side scenarios complete.")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python3 integration_tests.py <sock_path>")
        sys.exit(1)
    asyncio.run(run_tests(sys.argv[1]))
