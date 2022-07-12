# Pendulum Chain

Pendulum chain by SatoshiPay. More information about Pendulum can be found [here](https://pendulum.gitbook.io/pendulum-docs/).

### How to Run Tests
To run the unit tests, execute
```
cargo test
```

### How to Build
To build the project, execute
```
cargo b --release
```
A successful build will create a **target** folder with a sub folder **release**.
Check for `parachain-collator` in the **release** folder.

### How to Generate Chain Spec
There are 2 different [runtime](runtime)s currently in the repo; **amplitude** for the Amplitude parachain and **development** for the Pendulum parachain. Any of these runtimes are used depending on the config. The config is created by generating the chain spec:
```
./target/release/parachain-collator build-spec --disable-default-bootnode > local-parachain-plain.json
```
To create the amplitude spec, the `--chain` has to be specified:
```
./target/release/parachain-collator build-spec --chain amplitude --disable-default-bootnode > local-parachain-plain.json
```

For the raw chain spec, just add the option `--raw` and the `--chain` should be the one you just generated:
```
./target/release/parachain-collator build-spec --chain local-parachain-plain.json --raw --disable-default-bootnode > local-parachain-raw.json
```

### How to Generate Wasm:
```
/target/release/parachain-collator export-genesis-wasm --chain local-parachain-raw.json > para-2000-wasm
```
### How to Generate Genesis State:
```
./target/release/parachain-collator export-genesis-state --chain rococo-local-parachain-raw.json > para-2000-genesis
```

Note: The amplitude chain specs, the wasm and the genesis state are already available in the [res](res) folder.

### How to Run:
To run the collator, execute:
```
./target/release/parachain-collator
--collator \
--allow-private-ipv4 \
--unsafe-ws-external \
--rpc-cors all \
--rpc-external \
--rpc-methods Unsafe \
--name <INSERT_NAME> \
--ws-port <WS_PORT> --port <PARA_PORT> --rpc-port <RPC_PORT> \
--chain <PARA_SPEC_RAW.json> \
--execution=Native \
-- \
--port <RELAY_PORT>\
--chain <RELAY_SPEC_RAW.json> \
--execution=wasm --sync fast --pruning archive
```
An example for Amplitude will look like this:
```
./target/release/parachain-collator
--collator \
--allow-private-ipv4 \
--unsafe-ws-external \
--rpc-cors all \
--rpc-external \
--rpc-methods Unsafe \
--name amplitude-collator-1 \
--ws-port 9945 --port 30335 --rpc-port 9935 \
--chain res/amplitude-spec-raw.json \
--execution=Native \
-- \
--port 30334 \
--chain kusama-raw.json \
--execution=wasm --sync fast --pruning archive
```
For a local testing, you can replace `--name` with just `--alice` or `--bob`. You also need to specify the `--bootnode`.  Here's an example:
```
./target/release/parachain-collator \
--alice \
--rpc-cors=all \
--collator \
--force-authoring \
--chain rococo-local-parachain-2000-raw.json \
--base-path /tmp/parachain/alice \
--port 40333 \
--ws-port 8844 \
--enable-offchain-indexing TRUE \
-- \
--execution wasm \
--chain ./rococo-custom-2-raw.json \
--bootnodes /ip4/127.0.0.1/tcp/30333/p2p/<ALICE_NODE_ID> \
--port 30343 \
--ws-port 9977
```

There are prerequisites to run the collator with a local relay chain. Refer to [how to run Pendulum locally](https://pendulum.gitbook.io/pendulum-docs/build/running-pendulum-locally).
