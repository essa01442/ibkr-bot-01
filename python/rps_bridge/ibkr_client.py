import asyncio
import json
import logging
from ib_insync import IB, Stock, LimitOrder, MarketOrder, StopOrder

logger = logging.getLogger(__name__)

class IbkrClient:
    def __init__(self, host='127.0.0.1', port=7497, client_id=1,
                 command_socket_path='/var/run/rps/rps_commands.sock'):
        self.ib = IB()
        self.host = host
        self.port = port
        self.client_id = client_id
        self.command_socket_path = command_socket_path
        # symbol_id → IB Contract cache
        self._contract_cache: dict = {}

    async def connect(self):
        await self.ib.connectAsync(self.host, self.port, self.client_id)
        logger.info(f"Connected to IBKR at {self.host}:{self.port}")

    def subscribe_market_data(self, contract, symbol_id: int):
        self._contract_cache[symbol_id] = contract
        self.ib.reqMktData(contract, '', False, False)

    def get_contract(self, symbol_id: int):
        return self._contract_cache.get(symbol_id)

    async def place_limit_order(self, symbol_id: int, side: str, qty: int,
                                 limit_price: float, idempotency_key: str,
                                 stop_loss: float = None, take_profit: float = None):
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
            await asyncio.sleep(0)         # yield to event loop

            broker_id = str(trade.order.orderId)
            logger.info(f"Placed {action} {qty}@{limit_price} for sym={symbol_id} "
                        f"broker_id={broker_id} key={idempotency_key}")

            # Attach server-side stop loss bracket (spec §1: server-side stop required)
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

    async def cancel_order(self, broker_order_id: int):
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

    async def listen_for_commands(self):
        """
        Listens on a separate UDS socket for commands from Rust (cancel, place).
        Commands are newline-delimited JSON.
        Format: {"type": "cancel", "broker_order_id": 12345}
                {"type": "place", "symbol_id": 1, "side": "BUY", "qty": 100,
                 "limit_price": 5.50, "stop_loss": 5.40, "key": "..."}
        """
        import pathlib, os
        sock_dir = pathlib.Path(self.command_socket_path).parent
        sock_dir.mkdir(mode=0o700, parents=True, exist_ok=True)

        if pathlib.Path(self.command_socket_path).exists():
            os.unlink(self.command_socket_path)

        server = await asyncio.start_unix_server(
            self._handle_command_connection,
            path=self.command_socket_path
        )
        os.chmod(self.command_socket_path, 0o600)
        logger.info(f"Listening for commands on {self.command_socket_path}")
        async with server:
            await server.serve_forever()

    async def _handle_command_connection(self, reader, writer):
        try:
            while True:
                line = await reader.readline()
                if not line:
                    break
                cmd = json.loads(line.decode())
                if cmd['type'] == 'cancel':
                    await self.cancel_order(cmd['broker_order_id'])
                elif cmd['type'] == 'place':
                    await self.place_limit_order(
                        symbol_id=cmd['symbol_id'],
                        side=cmd['side'],
                        qty=cmd['qty'],
                        limit_price=cmd['limit_price'],
                        idempotency_key=cmd['key'],
                        stop_loss=cmd.get('stop_loss'),
                        take_profit=cmd.get('take_profit'),
                    )
        except Exception as e:
            logger.error(f"Command connection error: {e}")
        finally:
            writer.close()
