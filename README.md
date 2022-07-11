# Pendulum Chain

Pendulum chain by SatoshiPay.

Node runtime that is parachain ready (configured with cumulus).

🚧 This is a work in progress. 🚧

Based on Substrate. Repository based on [Substrate parachain template](https://github.com/substrate-developer-hub/substrate-parachain-template).

More information about Pendulum can be found [here](https://pendulum.gitbook.io/pendulum-docs/).
### Tests
To run the unit tests, execute `cargo test`.

### Building and running locally
Refer to the pendulum docs about [running the pendulum locally](https://pendulum.gitbook.io/pendulum-docs/build/running-pendulum-locally).

### Amplitude Chain
Amplitude is a canary network of the blockchain Pendulum. More information can be found in the [pendulum docs](https://pendulum.gitbook.io/pendulum-docs/amplitude-parachain/amplitude-and-ampe).

The chain spec, wasm, and genesis state are located in the [res folder](res).

Governance is applied in Amplitude. Here are the additional pallets added in its runtime:
* [sudo](https://paritytech.github.io/substrate/master/pallet_sudo/index.html)
* [democracy](https://paritytech.github.io/substrate/master/pallet_democracy/index.html)
* council [collective](https://paritytech.github.io/substrate/master/pallet_collective/index.html)
* technical committee collective
* [scheduler](https://paritytech.github.io/substrate/master/pallet_scheduler/index.html)
* [preimage](https://paritytech.github.io/substrate/master/pallet_preimage/index.html)
* [multisig](https://paritytech.github.io/substrate/master/pallet_multisig/index.html)

Some implementation differences between the Amplitude and Pendulum development:

|                |Amplitude            | Pendulum development                      |
|----------------|-------------------------------|-----------------------------|
|fees |distributed to the collators            |not handled           |
|identifier|57          |42          |
|session period          |4 hours|6 hours|
| max number of aura authorities | 200 | 100_000 |
| weight to fee calculation |  |

