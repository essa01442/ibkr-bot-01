import asyncio
import os
import sys
import time
import msgpack
import socket
import threading

def send_to_uds(path, data):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.connect(path)
        client.sendall(data)

def command_server_thread(cmd_path, cancel_event):
    if os.path.exists(cmd_path):
        os.unlink(cmd_path)

    server = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
    server.bind(cmd_path)

    while not cancel_event.is_set():
        server.settimeout(0.5)
        try:
            data, _ = server.recvfrom(4096)
            cmd = msgpack.unpackb(data)
            print(f"Python received command: {cmd}")
            if "CancelOrder" in cmd:
                print("Scenario 2: Received CancelOrder command in Python")
                # Usually we'd send an Ack back via UDS, but this confirms we got it
        except socket.timeout:
            continue
    server.close()

async def run_tests(sock_path):
    print("Starting integration tests on", sock_path)

    cmd_path = "/tmp/rps_test_commands.sock"
    cancel_event = threading.Event()
    cmd_thread = threading.Thread(target=command_server_thread, args=(cmd_path, cancel_event))
    cmd_thread.start()

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

    # Scenario 3: Order state reflection
    fill_event = {
        "ts_src": int(time.time() * 1_000_000),
        "ts_rx": int(time.time() * 1_000_000),
        "ts_proc": int(time.time() * 1_000_000),
        "seq": 2,
        "symbol_id": 42,
        "kind": {
            "Fill": {
                "order_id": 999,
                "price": 10.5,
                "size": 100,
                "side": "Bid",
                "liquidity": 0
            }
        }
    }
    send_to_uds(sock_path, msgpack.packb(fill_event))
    print("Sent Fill event (Scenario 3).")

    # Scenario 4: Malformed payload
    send_to_uds(sock_path, b'\xff\x00\x11\x22')
    print("Sent malformed payload.")

    # Scenario 5 & 6: Heartbeat missing, then reconnect
    heartbeat_event = {
        "ts_src": int(time.time() * 1_000_000),
        "ts_rx": int(time.time() * 1_000_000),
        "ts_proc": int(time.time() * 1_000_000),
        "seq": 3,
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

    cancel_event.set()
    cmd_thread.join()

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python3 integration_tests.py <sock_path>")
        sys.exit(1)
    asyncio.run(run_tests(sys.argv[1]))
