import asyncio
import msgpack
import socket

class UdsSender:
    def __init__(self, socket_path='/tmp/rps_uds.sock'):
        self.socket_path = socket_path
        self.writer = None

    async def connect(self):
        # Retry loop for connection
        while True:
            try:
                reader, writer = await asyncio.open_unix_connection(self.socket_path)
                self.writer = writer
                print(f"Connected to UDS at {self.socket_path}")
                break
            except (FileNotFoundError, ConnectionRefusedError):
                await asyncio.sleep(1)

    async def send_event(self, event):
        if not self.writer:
            return
        # Using msgpack for now as per guidance
        packed = msgpack.packb(event)
        try:
            self.writer.write(packed)
            await self.writer.drain()
        except Exception as e:
            print(f"Error sending event: {e}")
            self.writer = None
            await self.connect()
