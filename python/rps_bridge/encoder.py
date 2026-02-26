import msgpack
# import flatbuffers

class Encoder:
    @staticmethod
    def encode_msgpack(data):
        return msgpack.packb(data)

    @staticmethod
    def encode_flatbuffers(data):
        # Implement flatbuffer serialization
        pass
