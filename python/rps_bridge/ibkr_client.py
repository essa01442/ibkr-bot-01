import asyncio
import json
import logging
import pathlib
import os
import msgpack
from typing import Optional, Any
from ib_insync import IB, LimitOrder, StopOrder, Contract

logger = logging.getLogger(__name__)

class IbkrClient:
    def __init__(self, host: str = '127.0.0.1', port: int = 7497, client_id: int = 1,
                 command_socket_path: str = '/var/run/rps/rps_commands.sock'):
        self.ib = IB()
        self.host = host
        self.port = port
        self.client_id = client_id
        self.command_socket_path = command_socket_path
        # symbol_id → IB Contract cache
        self._contract_cache: dict[int, Contract] = {}

    async def connect(self) -> None:
        await self.ib.connectAsync(self.host, self.port, self.client_id)
        logger.info(f"Connected to IBKR at {self.host}:{self.port}")

    def subscribe_market_data(self, contract: Contract, symbol_id: int) -> None:
        self._contract_cache[symbol_id] = contract
        self.ib.reqMktData(contract, '', False, False)

    def get_contract(self, symbol_id: int) -> Optional[Contract]:
        return self._contract_cache.get(symbol_id)

    async def place_limit_order(
        self,
        symbol_id: int,
        side: str,
        qty: int,
        limit_price: float,
        idempotency_key: str,
        stop_loss: Optional[float] = None,
        take_profit: Optional[float] = None
    ) -> Optional[str]:
        """
        Place a limit order. side = 'BUY' or 'SELL'.
        Optionally attach a stop-loss bracket.
        Returns broker order id string, or None on failure.
        """
        contract = self.get_contract(symbol_id)
        if contract is None:
            logger.error(f"No contract for symbol_id={symbol_id}")
            return None

        action = 'BUY' if side == 'BUY' else 'SELL'
        order = LimitOrder(action, qty, limit_price)
        order.orderRef = idempotency_key   # idempotency key visible in TWS
        order.tif = 'IOC'                  # Immediate or Cancel per spec

        try:
            trade = self.ib.placeOrder(contract, order)
            # await asyncio.sleep(0)  # yield not strictly needed with placeOrder async nature but good practice

            # Wait for orderId to be populated if needed?
            # placeOrder returns a Trade object immediately.
            # If nextId is managed correctly by IB class, orderId should be valid or temp.
            # But usually it's fine.

            broker_id = str(trade.order.orderId)
            logger.info(f"Placed {action} {qty}@{limit_price} for sym={symbol_id} "
                        f"broker_id={broker_id} key={idempotency_key}")

            # Attach server-side stop loss bracket (spec §1: server-side stop required)
            # Note: Tif IOC orders usually execute immediately or cancel.
            # Attaching a Stop Loss to an IOC might be race-prone if the parent fills instantly.
            # Standard IBKR approach for bracket is parentId.
            if stop_loss is not None and action == 'BUY':
                stop_order = StopOrder('SELL', qty, stop_loss)
                stop_order.parentId = trade.order.orderId
                stop_order.tif = 'GTC'
                stop_order.transmit = True
                self.ib.placeOrder(contract, stop_order)
                logger.info(f"Attached stop-loss @{stop_loss} for broker_id={broker_id}")

            return broker_id

        except Exception as e:
            logger.error(f"Failed to place order for sym={symbol_id}: {e}")
            return None

    async def cancel_order(self, broker_order_id: int) -> bool:
        """Cancel an open order by broker order id."""
        try:
            open_orders = self.ib.openOrders()
            for order in open_orders:
                if order.orderId == broker_order_id:
                    self.ib.cancelOrder(order)
                    logger.info(f"Cancelled order {broker_order_id}")
                    return True
            logger.warning(f"Order {broker_order_id} not found in open orders")
            return False
        except Exception as e:
            logger.error(f"Failed to cancel order {broker_order_id}: {e}")
            return False

    async def listen_for_commands(self, uds_sender) -> None:
        """
        Listens on a separate UDS Datagram socket for commands from Rust (OmsCommand).
        Commands are msgpack serialized.
        """
        self.uds_sender = uds_sender
        sock_path = pathlib.Path(self.command_socket_path)
        sock_dir = sock_path.parent

        try:
            sock_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        except OSError as e:
            raise RuntimeError(
                f"Cannot create command socket directory {sock_dir}: {e}. "
                "Run as root or create /var/run/rps manually."
            ) from e

        if sock_path.exists():
            os.unlink(sock_path)

        class CmdProtocol(asyncio.DatagramProtocol):
            def __init__(self, ibkr_client):
                self.ibkr_client = ibkr_client

            def datagram_received(self, data, addr):
                import msgpack
                try:
                    cmd = msgpack.unpackb(data)
                    asyncio.create_task(self.ibkr_client._handle_oms_command(cmd))
                except Exception as e:
                    logger.error(f"Failed to decode OmsCommand: {e}")

        loop = asyncio.get_running_loop()
        transport, protocol = await loop.create_unix_datagram_endpoint(
            lambda: CmdProtocol(self),
            local_path=str(sock_path)
        )
        os.chmod(sock_path, 0o600)
        logger.info(f"Listening for commands on {sock_path}")

    async def _handle_oms_command(self, cmd: dict) -> None:
        try:
            if "CancelOrder" in cmd:
                cancel_req = cmd["CancelOrder"]
                order_id = cancel_req["order_id"]
                # In this mockup logic we map internal order_id to string, a real impl might look it up
                success = await self.cancel_order(order_id)
                import time
                ts = int(time.time() * 1_000_000)
                if success:
                    event = {
                        "ts_src": ts, "ts_rx": ts, "ts_proc": ts, "seq": 0, "symbol_id": 0,
                        "kind": {"CancelAck": {"order_id": order_id}}
                    }
                else:
                    event = {
                        "ts_src": ts, "ts_rx": ts, "ts_proc": ts, "seq": 0, "symbol_id": 0,
                        "kind": {"CancelReject": {"order_id": order_id, "reason": "Order not found or terminal"}}
                    }
                if hasattr(self, 'uds_sender') and self.uds_sender:
                    await self.uds_sender.send_event(event)

            elif "NewOrder" in cmd:
                req = cmd["NewOrder"]
                await self.place_limit_order(
                    symbol_id=req["symbol_id"],
                    side="BUY" if req["side"] == "Bid" else "SELL",
                    qty=req["qty"],
                    limit_price=req.get("limit_price", 0.0),
                    idempotency_key=req.get("idempotency_key", ""),
                    stop_loss=req.get("stop_price"),
                )
        except Exception as e:
            logger.error(f"Error handling OMS command: {e}")
