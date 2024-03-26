const { ApiPromise, WsProvider } = require("@polkadot/api");
const { Keyring } = require("@polkadot/api");
const readline = require("node:readline/promises");
const { stdin, stdout } = require("node:process");

const rl = readline.createInterface({ input: stdin, output: stdout });

// if keypair is undefined, then dryRun must be true
async function submitExtrinsic(transaction, keypair, dryRun) {
  console.log("Submit transaction");
  if (dryRun) {
    console.log("Dry run");
    console.log("Transaction size\n", (transaction.inner.toHex().length - 2) / 2);
    console.log("Transaction data:");
    console.log(transaction.inner.toHex());

    if (!dryRun) await multisigTransaction.signAndSend(initiatingSignatory);

    console.log("\n\nTransaction hash:", `0x${Buffer.from(transaction.inner.hash).toString("hex")}`);

    return;
  }

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

async function democracyProposal(call, type, deposit, submitPreimage, { api, submitterKeypair, dryRun }) {
  console.log("Preimage data", call.inner.toHex());
  console.log("Preimage hash", `0x${Buffer.from(call.inner.hash).toString("hex")}`);

  if (submitPreimage) {
    const submitPreimageTransaction = api.tx.preimage.notePreimage(call.inner.toHex());
    // await submitExtrinsic(submitPreimageTransaction, submitterKeypair, dryRun);
  }

  switch (type) {
    case "public":
      const submitProposalTransaction = api.tx.democracy.propose(call.inner.hash, deposit);
      // await submitExtrinsic(submitProposalTransaction, submitterKeypair, dryRun);
      break;

    case "external":
    case "externalMajority":
    case "externalDefault":
      let externalProposeTransaction;
      let threshold;

      const callLength = (call.inner.toHex().length - 2) / 2;

      switch (type) {
        case "external":
          externalProposeTransaction = api.tx.democracy.externalPropose({
            Lookup: { hash: call.inner.hash, len: callLength },
          });
          threshold = 3;
          break;
        case "externalMajority":
          externalProposeTransaction = api.tx.democracy.externalProposeMajority({
            Lookup: { hash: call.inner.hash, len: callLength },
          });
          threshold = 3;
          break;
        case "externalDefault":
          externalProposeTransaction = api.tx.democracy.externalProposeDefault({
            Lookup: { hash: call.inner.hash, len: callLength },
          });
          threshold = 5;
          break;
      }

      const councilTransaction = api.tx.council.propose(
        threshold,
        externalProposeTransaction,
        (externalProposeTransaction.toHex().length - 2) / 2
      );
      // await submitExtrinsic(councilTransaction, submitterKeypair, dryRun);
      break;
  }
}

function rawKeyString(rawKey) {
  return Array.from(rawKey)
    .map((entry) => entry.toString(16).padStart(2, "0"))
    .join("");
}

// if dryRun is true, then initiatingSignatory is the account address of the first signatory to submit the transaction
// if dryRun is false, then initiatingSignatory is the key pair of the first signatory to submit the transaction
async function multisig(transaction, signatories, initiatingSignatory, threshold, { api, keyring, dryRun }) {
  const submitterAddress = dryRun ? initiatingSignatory : initiatingSignatory.addressRaw;

  const otherSignatories = signatories
    .map((signer) => keyring.decodeAddress(signer))
    .filter((addressRaw) => rawKeyString(addressRaw) !== rawKeyString(submitterAddress))
    .sort((a, b) => (rawKeyString(a) < rawKeyString(b) ? -1 : 1));

  const multisigTransaction = api.tx.multisig.asMulti(threshold, otherSignatories, undefined, transaction, {
    ref_time: 1e9,
    proof_size: 1e5,
  });

  console.log("Transaction size\n", (transaction.inner.toHex().length - 2) / 2);
  console.log("Transaction data:");
  console.log(transaction.inner.toHex());

  if (!dryRun) {
    const result = await multisigTransaction.signAndSend(initiatingSignatory);
    console.log("\nResult of submission", JSON.stringify(result, null, 2));
  }

  console.log("\n\nTransaction hash:", `0x${Buffer.from(transaction.inner.hash).toString("hex")}`);
}

// if dryRun is true, then initiatingSignatory is the account address of the first signatory to submit the transaction
// if dryRun is false, then initiatingSignatory is the key pair of the first signatory to submit the transaction
async function simpleSudo(transaction, submitterKeypair, { api, keyring, dryRun }) {
  console.log("Transaction size\n", (transaction.inner.toHex().length - 2) / 2);
  console.log("Transaction data:");
  console.log(transaction.inner.toHex());

  if (!dryRun) {
    const result = await transaction.signAndSend(submitterKeypair);
    console.log("\nResult of submission", JSON.stringify(result, null, 2));
  }

  console.log("\n\nTransaction hash:", `0x${Buffer.from(transaction.inner.hash).toString("hex")}`);
}

function sudo(transaction, { api }) {
  return api.tx.sudo.sudoUncheckedWeight(transaction, 0);
}

exports.submitTransaction = async function (call, governanceMode, { api, definitions, unit, dryRun }) {
  if (governanceMode === "direct") {
    console.log("Transaction size\n", (call.inner.toHex().length - 2) / 2);
    console.log("Transaction data:");
    console.log(call.inner.toHex());
    console.log("\n\nTransaction hash:", `0x${Buffer.from(call.inner.hash).toString("hex")}`);
    return;
  }

  const keyring = new Keyring({ type: "sr25519" });

  const { signatoryAddresses, multisigThreshold } = definitions;

  let secretQuery;
  switch (governanceMode) {
    case "democracy":
      secretQuery = dryRun ? undefined : "Enter the secret mnemonic seed of the council member: ";
      break;

    case "sudo":
    case "simpleSudo":
      secretQuery = dryRun
        ? "Enter the address of the sudo signatory: "
        : "Enter the secret mnemonic seed of the sudo signatory: ";
      break;

    case "multisig":
      secretQuery = dryRun
        ? "Enter the address of the initiating signatory: "
        : "Enter the secret mnemonic seed of the multisig account signatory: ";
      break;
  }

  let submitterKeypair;

  if (secretQuery) {
    const submitterSecret = await rl.question(secretQuery);
    submitterKeypair = dryRun ? submitterSecret.trim() : keyring.addFromUri(submitterSecret.trim());
  }

  switch (governanceMode) {
    case "democracy":
      await democracyProposal(call, "externalMajority", unit, true, {
        api,
        submitterKeypair,
        dryRun,
      });
      break;

    case "sudo":
      {
        const sudoTransaction = sudo(call, { api });
        await multisig(sudoTransaction, signatoryAddresses.fixed, submitterKeypair, multisigThreshold, {
          api,
          keyring,
          dryRun,
        });
      }
      break;

    case "simpleSudo":
      {
        const sudoTransaction = sudo(call, { api });
        await simpleSudo(sudoTransaction, submitterKeypair, {
          api,
          keyring,
          dryRun,
        });
      }
      break;

    case "multisig":
      {
        await multisig(call, signatoryAddresses.fixed, submitterKeypair, multisigThreshold, {
          api,
          keyring,
          dryRun,
        });
      }
      break;
  }
};

exports.submitTransactionFor = async function (call, address, ss58Prefix, dryRun) {
  const keyring = new Keyring({ type: "sr25519", ss58Format: ss58Prefix });

  secretKey = await rl.question(`Enter the secret mnemonic seed of address "${address}": `);
  const keypair = keyring.addFromUri(secretKey.trim());
  if (keypair.address !== address) {
    throw new Error(`Incorrect private key for address: expected ${address}, got: ${keypair.address}`);
  }

  await submitExtrinsic(call, keypair, dryRun);
};


async function createRelayChainXcmTransaction(
  createInnerTransaction,
  maximumFeePaidOnRelayChain,
  { api, definitions: { paraId, relayChainWebsocketUrl } }
) {
  if (paraId === undefined) return undefined;
  if (relayChainWebsocketUrl === undefined) return undefined;

  const relayChainWsProvider = new WsProvider(relayChainWebsocketUrl);
  const relayChainApi = await ApiPromise.create({
    provider: relayChainWsProvider,
  });

  const innerTransaction = createInnerTransaction(relayChainApi);

  const innerCallEncoded = innerTransaction.inner.toU8a();
  const innerCallInfo = await relayChainApi.call.transactionPaymentCallApi.queryCallInfo(
    innerCallEncoded,
    innerCallEncoded.length
  );

  const destination = {
    v2: { parents: 1, interior: "Here" },
  };

  const transaction = api.tx.polkadotXcm.send(destination, {
    v2: [
      {
        WithdrawAsset: [
          {
            id: {
              Concrete: {
                parents: 0,
                interior: "Here",
              },
            },
            fun: {
              Fungible: maximumFeePaidOnRelayChain,
            },
          },
        ],
      },
      {
        BuyExecution: {
          fees: {
            id: {
              Concrete: {
                parents: 0,
                interior: "Here",
              },
            },
            fun: {
              Fungible: maximumFeePaidOnRelayChain,
            },
          },
          weightLimit: "Unlimited",
        },
      },
      {
        Transact: {
          originType: "Native",
          requireWeightAtMost: innerCallInfo.weight.refTime,
          call: {
            encoded: innerTransaction.inner.toHex(),
          },
        },
      },
      { RefundSurplus: {} },
      {
        DepositAsset: {
          assets: {
            Wild: "All",
          },
          maxAssets: 1,
          beneficiary: {
            parents: 0,
            interior: {
              X1: {
                Parachain: paraId,
              },
            },
          },
        },
      },
    ],
  });

  return transaction;
}


