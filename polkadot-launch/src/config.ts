// Collator flags
const flags = ["--unsafe-ws-external", "--force-authoring", "--", "--execution=wasm"];

const relay = {
    bin: "../../bin/polkadot",
    chain: "rococo-local",
    nodes: [
        {
            name: "alice",
            wsPort: 9944,
            port: 30444,
        },
        {
            name: "bob",
            wsPort: 9955,
            port: 30555,
        },
    ],
    genesis: {
        runtime: {
            runtime_genesis_config: {
                configuration: {
                    config: {
                        validation_upgrade_frequency: 10,
                        validation_upgrade_delay: 10,
                    },
                },
            },
        },
    },
};

const polkadot_collator = {
    bin: "polkadot-collator",
    id: "200",
    balance: "1000000",
    nodes: [
        {
            wsPort: 9988,
            port: 31200,
            name: "alice",
            flags,
        },
    ],
};

const pendulum_collator = {
    bin: "../../target/release/parachain-collator",
    id: "300",
    balance: "1000000000000000000000",
    nodes: [
        {
            wsPort: 9999,
            port: 31300,
            name: "alice",
            flags,
        },
    ],
};

export const config = {
    relaychain: relay,
    parachains: [pendulum_collator],
    types: {},
    finalization: false,
};
