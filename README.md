# Graphcast 3LA

[![Docs](https://img.shields.io/badge/docs-latest-brightgreen.svg)](https://docs.graphops.xyz/graphcast/radios/graphcast-3la)

## Introduction

This radio shall monitor the graphcast network by the pubsub topic of `graphcast-v0-[network-CAIP-chain-id]`. The radio will not send messages to the network, but it will record the messages and generate basic metrics for network monitoring. 

## Quick Start

- Running Postgres instance
- Set Postgres url to `DATABASE_URL` in `.env`
- General GraphcastAgent environmental variables are required to listen to the network, check SDK docs and cli help for specifics. Here is a brief list of required/recommended settings 
  - wallet_key: Graphcast id,
  - graph_node_endpoint: this is required for GraphcastAgent but can later be abstracted as it is not used in 3la operations,
  - graphcast_namespace: choose which graphcast network to listen ('mainnet', 'testnet'),
- `cargo run`

Incoming message type constraints:
- safisfy graphql output type
- json object: Serialize + DeserializeOwned

Example message table

| id | message                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
|----|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| 1  | {"nonce": 1686182179, "network": "mainnet", "payload": {"content": "0x3f...", "identifier": "QmVhiE4nax9i86UBnBmQCYDzvjWuwHShYh7aspGPQhU5Sj"}, "signature": "dff1...", "block_hash": "276e...", "identifier": "QmVhiE4nax9i86UBnBmQCYDzvjWuwHShYh7aspGPQhU5Sj", "block_number": 17431860} |
| 2  | {"nonce": 1686182183, "network": "goerli", "payload": {"content": "0xc0...", "identifier": "QmacQnSgia4iDPWHpeY6aWxesRFdb8o5DKZUx96zZqEWrB"}, "signature": "dbd2...", "block_hash": "0198...", "identifier": "QmacQnSgia4iDPWHpeY6aWxesRFdb8o5DKZUx96zZqEWrB", "block_number": 9140860} |
| ...|            ...                                         


This radio should monitor the graphcast network by the pubsub topic of `graphcast-v0-[network-CAIP-chain-id]`. The radio will not send messages to the network, but it will record the messages and generate basic metrics for network monitoring. 

## 🧪 Testing

To run unit tests for the Radio. We recommend using [nextest](https://nexte.st/) as your test runner. Once you have it installed you can run the tests using the following commands:

```
cargo nextest run
```

There's also integration tests, which you can run with the included bash script:

```
sh run-tests.sh
```

## Contributing

We welcome and appreciate your contributions! Please see the [Contributor Guide](/CONTRIBUTING.md), [Code Of Conduct](/CODE_OF_CONDUCT.md) and [Security Notes](/SECURITY.md) for this repository.
