import socket
import struct

import msgpack
import numpy as np


class BaseData:
    def __init__(self, sensor_data):
        if len(sensor_data) >= 2:
            self.sensor_1 = np.array(sensor_data[0])
            self.sensor_2 = np.array(sensor_data[1])
        else:
            raise ValueError("Not enough data for BaseData initialization")


class UnitData:
    def __init__(self, unit_data):
        if len(unit_data) >= 3:
            self.id = unit_data[0]
            self.sensor_1 = np.array(unit_data[1])
            self.sensor_2 = np.array(unit_data[2])
        else:
            raise ValueError("Not enough data for UnitData initialization")


class Message:
    def __init__(self, base_data, units_data):
        self.base = BaseData(base_data)
        self.units = [UnitData(unit) for unit in units_data]


class Response:
    def __init__(self, base_action, unit_actions):
        self.base = base_action
        self.units = unit_actions


def create_response(message):
    # This function creates a response based on the received message.
    # You might want to implement some logic based on the sensors' data.
    base_action = "NOP"  # Example action
    unit_actions = [{'id': unit.id, 'action': "MOVE"} for unit in message.units]
    return Response(base_action, unit_actions)


def send_response(client_socket, response: Response):
    # Serializing the response object to a MessagePack binary format
    response_data = {
        'base': response.base,
        'units': response.units
    }
    packed_data = msgpack.packb(response_data)
    # Prefix each message with its length packed in 4 bytes (big-endian format)
    length_prefix = struct.pack('>I', len(packed_data))
    print(f"packed data: {packed_data}")
    # Send the length of the packed data first
    client_socket.sendall(length_prefix)
    # Send the actual packed data
    client_socket.sendall(packed_data)
    print("Response sent back to client.")


def deserialize_message(data):
    if len(data) >= 1:
        base_data = data[0]  # Base data is the first element
        units_data = data[1]  # Assuming all other elements are unit data
        return Message(base_data, units_data)
    else:
        raise ValueError("Received data does not contain enough elements")


def receive_exact(sock, size):
    buffer = bytearray()
    while len(buffer) < size:
        part = sock.recv(size - len(buffer))
        if not part:
            raise ConnectionError("Connection closed prematurely")
        buffer.extend(part)
    return buffer


def start_server(port):
    server_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server_socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server_socket.bind(('0.0.0.0', port))
    server_socket.listen(5)
    print(f"Server is listening on port {port}")

    try:
        while True:
            client_socket, addr = server_socket.accept()
            print(f"Received connection from {addr}")
            while True:
                size_bytes = receive_exact(client_socket, 4)
                expected_size = int.from_bytes(size_bytes, 'big')

                # Now receive exactly that amount of data
                full_data = receive_exact(client_socket, expected_size)
                print(f"Received all expected {expected_size} bytes")

                if full_data:
                    try:
                        data = msgpack.unpackb(full_data, raw=False)
                        # print("Received data:")
                        # print("Raw data:", data)
                        message = deserialize_message(data)
                        print("Base Sensor 1:\n", message.base.sensor_1)
                        print("Base Sensor 2:\n", message.base.sensor_2)
                        for unit in message.units:
                            print(f"Unit ID {unit.id} Sensor 1:\n{unit.sensor_1}")
                            print(f"Unit ID {unit.id} Sensor 2:\n{unit.sensor_2}")
                    except Exception as e:
                        print(f"Failed to process data: {str(e)}")
                else:
                    print("No data received")

                    # Create and send a response
                response = create_response(message)
                send_response(client_socket, response)

    except Exception as e:
        print(f"Server error: {str(e)}")
        server_socket.close()



if __name__ == "__main__":
    port_number = 12345
    start_server(port_number)
