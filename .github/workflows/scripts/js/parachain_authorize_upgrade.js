// This code will submit a transaction to propose an `authorizeUpgrade`
// for Pendulum:
//  * save to env var PENDULUM_SEED the account seed of Pendulum
//  * save to env var PENDULUM_WASM_FILE the compressed wasm file of Pendulum
//
// for Amplitude:
//  * save to env var AMPLITUDE_SEED the account seed of Amplitude
//  * save to env var AMPLITUDE_WASM_FILE the compressed wasm file of Amplitude
//
// steps:
// 1. npm init -y
// 2. npm install
// 3. node parachain_authorize_upgrade.js <chain> <compressed.wasm>

const { ApiPromise, WsProvider, Keyring } = require("@polkadot/api");
const { blake2AsHex } = require("@polkadot/util-crypto");
const fs = require('fs');

const amplitudeDefinition = {
    websocketUrl: "wss://rpc-amplitude.pendulumchain.tech",
    accountSeed: process.env.AMPLITUDE_SEED,
    wasmFile: process.env.AMPLITUDE_WASM_FILE
}

const pendulumDefinition = {
    websocketUrl: "wss://rpc-pendulum.prd.pendulumchain.tech",
    accountSeed: process.env.PENDULUM_SEED,
    wasmFile: process.env.PENDULUM_WASM_FILE
}

// if keypair is undefined, then dryRun must be true
async function submitExtrinsic(transaction, keypair) {
    console.log("Submit transaction: ", transaction);

    await new Promise((resolve, reject) => {
        transaction.signAndSend(keypair, ({ status, dispatchError }) => {
            // status would still be set, but in the case of error we can shortcut
            // to just check it (so an error would indicate InBlock or Finalized)
            if (dispatchError) {
                if (dispatchError.isModule) {
                    // for module errors, we have the section indexed, lookup
                    const decoded = api.registry.findMetaError(dispatchError.asModule);
                    const { docs, name, section } = decoded;

                    console.log(`${section}.${name}: ${docs.join(" ")}`);
                } else {
                    // Other, CannotLookup, BadOrigin, no extra info
                    console.log(dispatchError.toString());
                }
                reject();
            }

            if (status.isInBlock) {
                console.log("Success: transaction in block");
                resolve();
            }

            if (status.isFinalized) {
                console.log("Transaction finalized");
            }
        });
    });
}

async function democracyProposal(call, { api, submitterKeypair }) {
    console.log("Preimage data", call.inner.toHex());
    console.log("Preimage hash", `0x${Buffer.from(call.inner.hash).toString("hex")}`);

    const submitPreimageTransaction = api.tx.preimage.notePreimage(call.inner.toHex());
    // uncomment if ready
    // await submitExtrinsic(submitPreimageTransaction, submitterKeypair);
}

async function submitTransaction(accountSeed, call, api) {
    const keyring = new Keyring({ type: "sr25519" });
    let submitterKeypair =  keyring.addFromUri(accountSeed);
    console.log("account: ", submitterKeypair.address);

    await democracyProposal(call, {
        api,
        submitterKeypair
    });
};

function getDefinitions (network) {
    switch (network) {
        case "amplitude":
            return amplitudeDefinition;
        case "pendulum":
            return pendulumDefinition;
    }
};

async function main() {
    const args = process.argv;

    if (args.length < 3 ) {
        console.error('Please provide the chain to upgrade');
        process.exit(1);
    }

    let { accountSeed, websocketUrl, wasmFile } = getDefinitions(args[2]);
    console.log("the url: ", websocketUrl);

    const wsProvider = new WsProvider(websocketUrl);

    const api = await ApiPromise.create({ provider: wsProvider });

    const wasmFileBytes = fs.readFileSync(wasmFile);
    const wasmFileHash = blake2AsHex(wasmFileBytes, 256);
    console.log('wasmfile: ', wasmFileHash);

    const authorizeUpgrade = api.tx.parachainSystem.authorizeUpgrade(wasmFileHash, true);

    await submitTransaction(accountSeed,authorizeUpgrade, api);

    process.exit();
}

main();
