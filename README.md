# Pendulum Chain

Pendulum chain by SatoshiPay. More information about Pendulum can be found [here](https://docs.pendulumchain.org/).

### How to Run Tests

[`mocktopus`](https://docs.rs/mocktopus/latest/mocktopus/) is nightly-only lib, and is used in testing.  
So to run the tests, override the toolchain with nightly; minimum version is `nightly-2024-05-30`.

```
cargo +nightly test
```

### How to Build

To build the project, execute

```
cargo build --release
```

A successful build will create a binary under `./target/release/pendulum-node`.

### How to Generate Chain Spec

There are 4 different [runtime](runtime)s currently in the repo; **amplitude** for the Amplitude parachain, **foucoco**
for the Foucoco testnet (running on Rococo), **pendulum** for the Pendulum parachain and **development** for the local
development. Any of these runtimes are used depending on the config. The config is created by generating the chain spec:

```
./target/release/pendulum-node build-spec --disable-default-bootnode > local-parachain-plain.json
```

To create the amplitude spec, the `--chain` must be provided (similar for `pendulum` and `foucoco`):

```
./target/release/pendulum-node build-spec --chain amplitude --disable-default-bootnode > local-parachain-plain.json
```

For the raw chain spec, just add the option `--raw` and the `--chain` should be the one you just generated:

```
./target/release/pendulum-node build-spec --chain local-parachain-plain.json --raw --disable-default-bootnode > local-parachain-raw.json
```

### How to Generate Wasm:

```
./target/release/pendulum-node export-genesis-wasm --chain local-parachain-raw.json > para-2000-wasm
```

### How to Generate Genesis State:

```
./target/release/pendulum-node export-genesis-state --chain local-parachain-raw.json > para-2000-genesis
```

Note: The amplitude chain specs, the wasm and the genesis state are already available in the [res](res) folder.

### How to Run:

To run the collator, execute:

```
./target/release/pendulum-node
--collator \
--allow-private-ipv4 \
--unsafe-ws-external \
--rpc-cors all \
--rpc-external \
--rpc-methods Unsafe \
--name <ASSIGN_A_NAME> \
--ws-port <P_WS_PORT> --port <P_PORT> --rpc-port <P_RPC_PORT> \
--chain <P_SPEC_RAW.json> \
--execution=Native \
-- \
--port <R_PORT>\
--chain <R_SPEC_RAW.json> \
--execution=wasm --sync fast --pruning archive
```

where:
| Parachain | Relay Chain | Description |
|-------------------|-------------------|------------------------------------------|
| `ASSIGN_A_NAME` | | assigning a name to the chain |
| `P_WS_PORT` | | listening port for WebSocket connections |
| `P_PORT` | `R_PORT` | port for peer-to-peer communication |
| `P_RPC_PORT` | | port for remote procedure calls |
| `P_SPEC_RAW.json` | `R_SPEC_RAW.json` | raw json file of the chain spec |

An example for Amplitude will look like this:

```
./target/release/pendulum-node
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
--chain kusama.json \
--execution=wasm --sync fast --pruning archive
```

You can find the
kusama.json [here](https://github.com/paritytech/polkadot/blob/master/node/service/chain-specs/kusama.json).

For local testing, you can replace `--name` with predefined keys like `--alice` or `--bob`. You also need to specify
the `--bootnode`. Here's an example:

```
./target/release/pendulum-node \
--alice \
--rpc-cors=all \
--collator \
--force-authoring \
--chain local-parachain-raw.json \
--base-path /tmp/parachain/alice \
--port 40333 \
--ws-port 8844 \
--enable-offchain-indexing TRUE \
-- \
--execution wasm \
--chain rococo-custom-2-raw.json \
--bootnodes /ip4/127.0.0.1/tcp/30333/p2p/<ALICE_NODE_ID> \
--port 30343 \
--ws-port 9977
```

where `ALICE_NODE_ID` is the peer id of Alice.
You can find the
rococo-custom-2-raw.json [here](https://github.com/substrate-developer-hub/substrate-docs/blob/314e9cd3bd0ca9426bbfd381b79c3ef4d06b49c2/static/assets/tutorials/cumulus/chain-specs/rococo-custom-2-raw.json).

There are prerequisites in running the collator with a local relay chain. Refer
to [how to run Pendulum locally](https://pendulum.gitbook.io/pendulum-docs/build/build-environment/local-pendulum-chain-setup).

## How to benchmark runtime pallets

Build the node with the `production` profile and the `runtime-benchmarks` feature enabled.

```shell
cargo build --profile=production --features runtime-benchmarks --package pendulum-node
```

Run the benchmarks of a registered pallet.
The pallet has to be added to the list of defined benchmarks that you can find in the `benches` module of each
runtimes `lib.rs` file.

#### Pendulum

```shell
./target/production/pendulum-node benchmark pallet \
    --chain pendulum \
    --execution=wasm \
    --wasm-execution=compiled \
    --pallet "*" \
    --extrinsic "*" \
    --steps 50 \
    --repeat 20 \
    --output runtime/pendulum/src/weights/ \
    --template .maintain/frame-weight-template.hbs
```

#### Amplitude

```shell
./target/production/pendulum-node benchmark pallet \
    --chain amplitude \
    --execution=wasm \
    --wasm-execution=compiled \
    --pallet "*" \
    --extrinsic "*" \
    --steps 50 \
    --repeat 20 \
    --output runtime/amplitude/src/weights/ \
    --template .maintain/frame-weight-template.hbs
```

#### Foucoco

```shell
./target/production/pendulum-node benchmark pallet \
    --chain foucoco \
    --execution=wasm \
    --wasm-execution=compiled \
    --pallet "*" \
    --extrinsic "*" \
    --steps 50 \
    --repeat 20 \
    --output runtime/foucoco/src/weights/ \
    --template .maintain/frame-weight-template.hbs
```

### How to run in standalone mode

For testing purposes, it is useful to run an instance of the chain locally with the same runtime as the live chain, but using the instant-seal consensus mechanism. This means that upon every successful extrinsic, a new block will be produced instantly.

Currently we only support the use of the foucoco runtime to run the chain in this standalone mode.

To run: 

```shell
cargo run -p pendulum-node -- --chain {x}-spec-raw.json --instant-seal
```

Most other flags are irrelevant when instant-seal is used, and should not be passed, but it is still possible 
to use the port definition flags to change the default ports used.

#### Run with a specific genesis config

The previous command assumes there exist already a `...spec-raw.json` which defines the genesis config. The process
of creating this file is the same as the one described above in section [How to Generate Genesis State](#how-to-generate-chain-spec)

To run with the default standalone genesis config defined in `node/chain_spec.rs` (`foucoco_standalone_config()` function) use:

```shell
cargo run -p pendulum-node -- --chain res/foucoco-standalone-spec-raw.json --instant-seal
```

or simply:

```shell
cargo run -p pendulum-node -- --chain foucoco-standalone --instant-seal
```
