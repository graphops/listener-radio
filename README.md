# Graphcast 3LA

[![Docs](https://img.shields.io/badge/docs-latest-brightgreen.svg)](https://docs.graphops.xyz/graphcast/radios/graphcast-3la)

## Introduction

This radio monitors Graphcast network through subscribing to gossip network topics. The radio will not send messages to the network, but it will record the messages and generate basic metrics for network monitoring. 

## Quick Start

- Ensure a running Postgres instance
- Set Postgres url to `DATABASE_URL` in `.env`
- Set general GraphcastAgent environmental variables. Check SDK docs and cli help for specifics. Here is a brief list of required/recommended settings 
  - wallet_key: Graphcast id,
  - graph_node_endpoint: this is required for GraphcastAgent but can later be abstracted as it is not used in 3la operations,
  - graphcast_namespace: choose which graphcast network to listen ('mainnet', 'testnet'),
- `cargo run` from source code or build docker image

## Motivation

Graphcast network is a complex system with numerous nodes and connections, and monitoring it is crucial for maintaining its performance, identifying potential issues, and ensuring its robustness and reliability.

- Performance Optimization: to identify bottlenecks and areas of inefficiency.
- Troubleshooting: to quickly diagnose issues within the network, reducing downtime and improving reliability.
- Security: to immediately detect any unusual activity that might indicate a security breach.
- Planning and Forecasting: Record valuable data that can be used for planning and forecasting purposes, helping us to make informed decisions about the network's future.

Basic functions

- Data storage: Stores data of interest.
- API: Easy manipulation and management of data stored.
- Metrics Collection: collects various metrics about the network, such as the number of active nodes, the amount of messages/data being transferred, and the network's validity. Later it should track performances like latency.
- Logging: provides logs on network activity.

Future functions
- Error Detection: Detect and log errors in the network.
- Alerting: Send alerts when certain conditions are met, such as when the network's performance drops below a certain threshold.
- Advanced Analytics: More advanced analytics capabilities for more in-depth analysis of the network's performance.
- Integration with Other Tools: Compatibility with other tools in the network for more comprehensive network monitoring.
- UI Improvements: Generally more user-friendly and intuitive.

### Database

The tool comes with auto-migration for the database, but requires the user to create a valid DB connection. Make sure the database url passed in is valid.

Incoming message type constraints:
- satisfy GraphQL output type
- Serializeable and Deserializeable json object

Example message table

| id | message                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
|----|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| 1  | {"nonce": 1686182179, "network": "mainnet", "payload": {"content": "0x3f...", "identifier": "QmVhiE4nax9i86UBnBmQCYDzvjWuwHShYh7aspGPQhU5Sj"}, "signature": "dff1...", "block_hash": "276e...", "identifier": "QmVhiE4nax9i86UBnBmQCYDzvjWuwHShYh7aspGPQhU5Sj", "block_number": 17431860} |
| 2  | {"nonce": 1686182183, "network": "goerli", "payload": {"content": "0xc0...", "identifier": "QmacQnSgia4iDPWHpeY6aWxesRFdb8o5DKZUx96zZqEWrB"}, "signature": "dbd2...", "block_hash": "0198...", "identifier": "QmacQnSgia4iDPWHpeY6aWxesRFdb8o5DKZUx96zZqEWrB", "block_number": 9140860} |
| ...|            ...                                         


## 🧪 Testing

To run unit tests for the Radio. We recommend using [nextest](https://nexte.st/) as your test runner. Once you have it installed you can run the tests using the following commands:

```
cargo nextest run
```

## Contributing

We welcome and appreciate your contributions! Please see the [Contributor Guide](/CONTRIBUTING.md), [Code Of Conduct](/CODE_OF_CONDUCT.md) and [Security Notes](/SECURITY.md) for this repository.
