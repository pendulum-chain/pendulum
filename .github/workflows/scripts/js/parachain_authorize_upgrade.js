// This code will submit a transaction to propose an `authorizeUpgrade`
// steps:
// 1. npm init -y
// 2. npm install
// 3. node parachain_authorize_upgrade.js <chain> <compressed.wasm>

const { ApiPromise, WsProvider, Keyring } = require("@polkadot/api");
const { blake2AsHex } = require("@polkadot/util-crypto");
const fs = require('fs');

const { submitTransaction } = require("./common");

const { getDefinitions } = require("./constants");

const GOVERNANCE_MODE = "democracy"; // "democracy" | "sudo" | "multisig"
const DRY_RUN = true;


async function main() {
    const args = process.argv;

    if (args.length < 4 ) {
        console.error('Expecting two arguments!');
        process.exit(1);
    }

    const definitions = getDefinitions(args[2]);
    const { websocketUrl, distributionAccounts, otherParaIds, unit } = definitions;

    const wsProvider = new WsProvider(websocketUrl);
    const api = await ApiPromise.create({ provider: wsProvider });

    const wasmFileBytes = fs.readFileSync(args[3]);
    const wasmFileHash = blake2AsHex(wasmFileBytes, 256);
    console.log('wasmfile: ', wasmFileHash);

    const authorizeUpgrade = api.tx.parachainSystem.authorizeUpgrade(wasmFileHash, true);

    await submitTransaction(authorizeUpgrade, GOVERNANCE_MODE, { definitions, api, dryRun: DRY_RUN });

    process.exit();
}

main();
