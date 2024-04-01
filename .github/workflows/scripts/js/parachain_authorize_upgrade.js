const { ApiPromise, WsProvider, Keyring } = require("@polkadot/api");
const { blake2AsHex } = require("@polkadot/util-crypto");
const readline = require("node:readline/promises");
const { stdin, stdout } = require("node:process");
const fs = require('fs');

const rl = readline.createInterface({ input: stdin, output: stdout });

// if keypair is undefined, then dryRun must be true
async function submitExtrinsic(transaction, keypair) {
    console.log("Submit transaction: ", transaction);

    // await new Promise((resolve, reject) => {
    //     transaction.signAndSend(keypair, ({ status, dispatchError }) => {
    //         // status would still be set, but in the case of error we can shortcut
    //         // to just check it (so an error would indicate InBlock or Finalized)
    //         if (dispatchError) {
    //             if (dispatchError.isModule) {
    //                 // for module errors, we have the section indexed, lookup
    //                 const decoded = api.registry.findMetaError(dispatchError.asModule);
    //                 const { docs, name, section } = decoded;
    //
    //                 console.log(`${section}.${name}: ${docs.join(" ")}`);
    //             } else {
    //                 // Other, CannotLookup, BadOrigin, no extra info
    //                 console.log(dispatchError.toString());
    //             }
    //             reject();
    //         }
    //
    //         if (status.isInBlock) {
    //             console.log("Success: transaction in block");
    //             resolve();
    //         }
    //
    //         if (status.isFinalized) {
    //             console.log("Transaction finalized");
    //         }
    //     });
    // });
}

async function democracyProposal(call, { api, submitterKeypair }) {
    console.log("Preimage data", call.inner.toHex());
    console.log("Preimage hash", `0x${Buffer.from(call.inner.hash).toString("hex")}`);

    const submitPreimageTransaction = api.tx.preimage.notePreimage(call.inner.toHex());
    await submitExtrinsic(submitPreimageTransaction, submitterKeypair);

    const callLength = (call.inner.toHex().length - 2) / 2;
    const externalProposeTransaction = api.tx.democracy.externalProposeMajority({
        Lookup: { hash: call.inner.hash, len: callLength },
    });
    const threshold = 3;

    const councilTransaction = api.tx.council.propose(
        threshold,
        externalProposeTransaction,
        (externalProposeTransaction.toHex().length - 2) / 2
    );
    await submitExtrinsic(councilTransaction, submitterKeypair);
}

async function submitTransaction(call, api) {
    const keyring = new Keyring({ type: "sr25519" });

    // todo: need an account
    let secretQuery = "Enter the secret mnemonic seed of the council member: ";

    const submitterSecret = await rl.question(secretQuery);
    let submitterKeypair =  keyring.addFromUri(submitterSecret.trim());

    await democracyProposal(call, {
        api,
        submitterKeypair
    });
};

function getUrl (network) {
    switch (network) {
        case "amplitude":
            return "wss://rpc-amplitude.pendulumchain.tech";

        case "pendulum":
            return "wss://rpc-pendulum.prd.pendulumchain.tech";
    }
};


async function main() {
    const args = process.argv;

    if (args.length < 4 ) {
        console.error('Expecting two arguments!');
        process.exit(1);
    }

    console.log("the argument: ", args[2]);
    const websocketUrl = getUrl(args[2]);

    console.log("the url: ", websocketUrl);

    const wsProvider = new WsProvider(websocketUrl);

    const api = await ApiPromise.create({ provider: wsProvider });

    const wasmFileBytes = fs.readFileSync(args[3]);
    const wasmFileHash = blake2AsHex(wasmFileBytes, 256);
    console.log('wasmfile: ', wasmFileHash);

    const authorizeUpgrade = api.tx.parachainSystem.authorizeUpgrade(wasmFileHash, true);

    await submitTransaction(authorizeUpgrade, api);

    process.exit();
}

main();
