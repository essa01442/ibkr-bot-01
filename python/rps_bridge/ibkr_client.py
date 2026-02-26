from ib_insync import IB, Stock

class IbkrClient:
    def __init__(self, host='127.0.0.1', port=7497, client_id=1):
        self.ib = IB()
        self.host = host
        self.port = port
        self.client_id = client_id

    async def connect(self):
        await self.ib.connectAsync(self.host, self.port, self.client_id)

    def subscribe_market_data(self, contract):
        self.ib.reqMktData(contract, '', False, False)
