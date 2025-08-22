# TCP Drawing

A simple collaborative drawing application that works over TCP/IP networks. Users can draw shapes in real-time, with changes synchronized between all connected clients.

## Features

- Client-server architecture over TCP
- Real-time collaborative drawing
- Automatic role detection (server or client)
- Visual distinction between server (red) and client (green) drawings
- Dynamic shape sizing while drawing

## Libraries Used

- **macroquad (0.4.14)**: Graphics rendering and user input handling
- **dashmap (7.0.0-rc2)**: Thread-safe concurrent hash map for storing drawing entities
- **serde (1.0.219)**: Serialization/deserialization framework for network communication
- **serde_json (1.0)**: JSON support for serde
- **crossbeam-channel (0.5.15)**: Multi-producer multi-consumer channels for thread communication

## How to Use

Run as a server:
```
cargo run
```

Run as a client (connecting to a specific server):
```
cargo run <server_address:port>
```

By default, the application tries to bind to `127.0.0.1:8090`. If binding fails, it assumes the role of a client and attempts to connect to that address.

## Controls

- **Left Mouse Button**: Click and hold to draw shapes
- The size of shapes decreases as you continue drawing