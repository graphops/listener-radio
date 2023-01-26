# Graphcast 3LA

[![Docs](https://img.shields.io/badge/docs-latest-brightgreen.svg)](https://docs.graphops.xyz/graphcast/radios/graphcast-3la)

## Introduction

This radio shall monitor the graphcast network by the pubsub topic of `graphcast-v0-[network-CAIP-chain-id]`. The radio will not send messages to the network, but it will record the messages and generate basic metrics for network monitoring. 

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
