// Collator flags
const flags = ["--unsafe-rpc-external", "--unsafe-ws-external", "--rpc-cors=all", "--force-authoring", "--", "--execution=wasm"];

const relay = {
    bin: "../../bin/polkadot",
    chain: "rococo-local",
    nodes: [
        {
            name: "alice",
            port: 10000,
            rpcPort: 20000,
            wsPort: 30000,
        },
        {
            name: "bob",
            port: 10001,
            rpcPort: 20002,
            wsPort: 30003,
        },
    ],
    // genesis: {
    //     runtime: {
    //         runtime_genesis_config: {
    //             configuration: {
    //                 config: {
    //                     validation_upgrade_frequency: 10,
    //                     validation_upgrade_delay: 10,
    //                 },
    //             },
    //         },
    //     },
    // },
};

const collators = {
    pendulum: {
        bin: "../../bin/polkadot-collator",
        id: "200",
        balance: "1000000",
        nodes: [
            {
                name: "alice",
                port: 10002,
                rpcPort: 20002,
                wsPort: 30002,
                flags,
            },
        ],
    },
    cumulus: {
        bin: "../../target/release/parachain-collator",
        id: "300",
        balance: "1000000",
        nodes: [
            {
                name: "alice",
                port: 10003,
                rpcPort: 20003,
                wsPort: 30003,
                flags,
            },
        ],
    }
}

export const config = {
    relaychain: relay,
    parachains: [collators.pendulum],
    types: {},
    finalization: false,
};
