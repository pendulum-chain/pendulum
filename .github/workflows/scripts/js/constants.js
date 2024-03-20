const {
  foucocoSignatories,
  amplitudeSignatories,
  pendulumSignatories,
} = require("./signatories.json");

const localDefinition = {
  websocketUrl: "ws://127.0.0.1:9944",
  relayChainWebsocketUrl: undefined,
  genesisAccount: "6hESxBrhZ9ThDDsB1kGWpzj1jc3RMeb6QTGuHx6cb3F4YH2S",
  secondsPerBlock: 6,
  multisigThreshold: 3,
  signatoryAddresses: foucocoSignatories,
  existentialDeposit: 500n,
  unit: 10n ** 12n,
  distributionAccounts: {
    crowdloanReserve: "6nHM6jraY47FQqKvCY6yHGDiB2U1JMhhKbKeDv2DCc68A5Gh",
    ecosystemDevelopment: "6hZFMaTFbRkqwuZ7B4zEDWUQe11UvAE4KbPNsTjZEu13w9vz",
    liquidityIncentives: "6nAgoYasDAzqYZVN67zwWUgNdTcTV4bKDhYmqs1iaDXcKeLZ",
    protocolInitiatives: "6hpW4pn1pMoaPwKGwzbDNuv6v7tD8fnWqpMzkYaiUNNZdA23",
    marketing: "6kYPzPsBbNun6LXrTbq9UwZPh7sn6zQkX1p8BezX54BqEGqH",
  },
  sudo: "6ftBYTotU4mmCuvUqJvk6qEP7uCzzz771pTMoxcbHFb9rcPv",
  initialStakingCollators: [],
  ss58Prefix: 42,
  paraId: undefined,
  otherParaIds: undefined,
};

const foucocoDefinition = {
  websocketUrl: "wss://rpc-foucoco.pendulumchain.tech",
  relayChainWebsocketUrl: "wss://rococo-rpc.polkadot.io",
  // Moonbase Alpha urls
  // websocketUrl: "wss://moonbeam-00.pendulumchain.tech",
  // relayChainWebsocketUrl: "wss://frag-moonbase-relay-rpc-ws.g.moonbase.moonbeam.network",
  genesisAccount: "6hESxBrhZ9ThDDsB1kGWpzj1jc3RMeb6QTGuHx6cb3F4YH2S",
  secondsPerBlock: 12,
  multisigThreshold: 3,
  signatoryAddresses: foucocoSignatories,
  existentialDeposit: 1_000_000_000n,
  unit: 10n ** 12n,
  relaychainUnit: 10n ** 12n,
  distributionAccounts: {
    crowdloanReserve: "6nHM6jraY47FQqKvCY6yHGDiB2U1JMhhKbKeDv2DCc68A5Gh",
    ecosystemDevelopment: "6hZFMaTFbRkqwuZ7B4zEDWUQe11UvAE4KbPNsTjZEu13w9vz",
    liquidityIncentives: "6nAgoYasDAzqYZVN67zwWUgNdTcTV4bKDhYmqs1iaDXcKeLZ",
    protocolInitiatives: "6hpW4pn1pMoaPwKGwzbDNuv6v7tD8fnWqpMzkYaiUNNZdA23",
    marketing: "6kYPzPsBbNun6LXrTbq9UwZPh7sn6zQkX1p8BezX54BqEGqH",
  },
  initialStakingCollators: [
    "6ihktBwyFJYjE1LKdqoAWzo5VDPJJGso9D5iASZyhuN5JvGH",
    "6mbXa9Qca6B6cX51cbtfWWLhup84rMoMFCxNHjso15GBFyGh",
    "6mMdv2wmb4Cp8PAtDLF1WTh1wLPwPbETwtcjqgJLskdB8EYo",
    "6kL1dzcBJiLgMdAT1qDFD79CLupX1gCCF8RSg5Dh5qRgQeCx",
  ],
  ss58Prefix: 57,
  paraId: 2124,
  otherParaIds: {
    statemine: 1000,
    moonbase: 1000,
    bifrost: 2030,
  },
};

const amplitudeDefinition = {
  websocketUrl: "wss://rpc-amplitude.pendulumchain.tech",
  relayChainWebsocketUrl: "wss://kusama-rpc.dwellir.com",
  genesisAccount: "6nCzN2oTHkm5VV5CW2q9StXtd3J4CpuRHtQaYiBssCLxq6Dv",
  secondsPerBlock: 12,
  multisigThreshold: 3,
  signatoryAddresses: amplitudeSignatories,
  existentialDeposit: 1_000_000_000n,
  unit: 10n ** 12n,
  relaychainUnit: 10n ** 12n,
  distributionAccounts: {
    crowdloanReserve: "6kkrogmET4ULqTYXWa8UVqhaYgZMTEf2C7MgQtpFH3CXB783",
    ecosystemDevelopment: "6kw7NZVJMWh6fgwUSzzSUhf9pQSxBXCzJPrm7cRrN3ZzWuJS",
    liquidityIncentives: "6i6pdBXuzNH9m2bM4ipdiMGWSpfz3uDMriHCBdAVXTtzBNPA",
    protocolInitiatives: "6hj43L8TpPqFTLuyUTVqfQJPow57oi1ArNTJs5spVYAkFVqJ",
    marketing: "6kToZCN5iATwXSR3CKQeHDMcR543FMDUtp1fr5AA9RK6mygn",
  },
  initialStakingCollators: [
    "6mTATq7Ug9RPk4s8aMv5H7WVZ7RvwrJ1JitbYMXWPhanzqiv",
    "6n8WiWqjEB8nCNRo5mxXc89FqhuMd2dgXNSrzuPxoZSnatnL",
    "6ic56zZmjqo746yifWzcNxxzxLe3pRo8WNitotniUQvgKnyU",
    "6gvFApEyYj4EavJP26mwbVu7YxFBYZ9gaJFB7gv5gA6vNfze",
    "6mz3ymVAsfHotEhHphVRvLLBhMZ2frnwbuvW5QZiMRwJghxE",
    "6mpD3zcHcUBkxCjTsGg2tMTfmQZdXLVYZnk4UkN2XAUTLkRe",
    "6mGcZntk59RK2JfxfdmprgDJeByVUgaffMQYkp1ZeoEKeBJA",
    "6jq7obxC7AxhWeJNzopwYidKNNe48cLrbGSgB2zs2SuRTWGA",
  ],
  ss58Prefix: 57,
  paraId: 2124,
  otherParaIds: {
    statemine: 1000,
    bifrost: 2001,
    picasso: 2087,
  },
};

const pendulumDefinition = {
  websocketUrl: "wss://rpc-pendulum.prd.pendulumchain.tech",
  relayChainWebsocketUrl: "wss://polkadot-rpc.dwellir.com",
  genesisAccount: "6cY3Zrb2gr1xt3BczzJ3xoMpF7UyrcGNfR3cjkjcF7auq2Y9",
  secondsPerBlock: 12,
  multisigThreshold: 4,
  signatoryAddresses: pendulumSignatories,
  existentialDeposit: 1_000_000_000n,
  unit: 10n ** 12n,
  relaychainUnit: 10n ** 10n, // strange but that's how it is
  distributionAccounts: {
    genesis: "6cY3Zrb2gr1xt3BczzJ3xoMpF7UyrcGNfR3cjkjcF7auq2Y9",
    team: "6gfLdZvfW2w6fDaPpVfUs53W8Aay17s1bjwcFaqDaBVt7Muo",
    crowdloanReserve: "6biLQnLREwRd9aSPiN9xxR2UDCPa1XL3ZSwqNUxNEr3QvGDk",
    liquidityIncentives: "6eiGivQB9dtQUMs1VpxATipDYrewWSr4kGsvgjELgqnvRYyx",
    marketing: "6gKuTtzLBtgYyW3SP6jh7DnXbNU8fDVFG2AxHCLbGYqaspe7",
    treasury: "6dZRnXfN7nnrAUDWykWc7gpHpByVBj9HTRpFNNQyENh11xjq",
  },
  initialStakingCollators: [
    "6gUmMnikYxEkk4H7RdnsLRrzNRuDrGAh8JgSiCghG39qenX9",
    "6cgKZANaeUJ42VC7iAXrTzX8NC2gdn4WmYAHRo1RBjBfVvnk",
    "6bh2t6KMJ9BKgCs1B6qcrp5BjMyv2azmgBC6ySwZ3wrTeW5s",
    "6bBH94XAkscX5Q1oswuPSenUzjb9f2iPcfhTKdu1XCK1uwVS",
    "6emSrvAgGZXGBu255njQg3pBxDyQN47T7H2XDZuS5V5epHaX",
    "6fciE2ek1AMFUaFm4nizaHEZtXBy6eRxEcoygr3SFKfddBBK",
    "6ftBtHvYrThAv1xHYDnYrm2qQLFcj2rhkaU5GqNuqvKp57v6",
    "6feqfoP5htFpSriTd9oomDa1dZDmcM4XpjKEq8dfdcADCfGt",
  ],
  ss58Prefix: 56,
  paraId: 2094,
  otherParaIds: {
    statemint: 1000,
    moonbeam: 2004,
    bifrost: 2030,
    equilibrium: 2011,
    polkadex: 2040,
  },
};

exports.getDefinitions = function (network) {
  switch (network) {
    case "local":
      return localDefinition;

    case "foucoco":
      return foucocoDefinition;

    case "amplitude":
      return amplitudeDefinition;

    case "pendulum":
      return pendulumDefinition;
  }
};
