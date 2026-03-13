import msgpack

class Encoder:
    @staticmethod
    def encode_msgpack(data):
        return msgpack.packb(data)
