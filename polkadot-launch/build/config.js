var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __markAsModule = (target) => __defProp(target, "__esModule", { value: true });
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __reExport = (target, module2, copyDefault, desc) => {
  if (module2 && typeof module2 === "object" || typeof module2 === "function") {
    for (let key of __getOwnPropNames(module2))
      if (!__hasOwnProp.call(target, key) && (copyDefault || key !== "default"))
        __defProp(target, key, { get: () => module2[key], enumerable: !(desc = __getOwnPropDesc(module2, key)) || desc.enumerable });
  }
  return target;
};
var __toCommonJS = /* @__PURE__ */ ((cache) => {
  return (module2, temp) => {
    return cache && cache.get(module2) || (temp = __reExport(__markAsModule({}), module2, 1), cache && cache.set(module2, temp), temp);
  };
})(typeof WeakMap !== "undefined" ? /* @__PURE__ */ new WeakMap() : 0);
var config_exports = {};
__export(config_exports, {
  config: () => config
});
const flags = ["--unsafe-ws-external", "--force-authoring", "--", "--execution=wasm"];
const relay = {
  bin: "../../bin/polkadot",
  chain: "rococo-local",
  nodes: [
    {
      name: "alice",
      wsPort: 9944,
      port: 30444
    },
    {
      name: "bob",
      wsPort: 9955,
      port: 30555
    }
  ],
  genesis: {
    runtime: {
      runtime_genesis_config: {
        configuration: {
          config: {
            validation_upgrade_frequency: 10,
            validation_upgrade_delay: 10
          }
        }
      }
    }
  }
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
      flags
    }
  ]
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
      flags
    }
  ]
};
const config = {
  relaychain: relay,
  parachains: [pendulum_collator],
  types: {},
  finalization: false
};
module.exports = __toCommonJS(config_exports);
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  config
});
