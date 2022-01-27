var __async = (__this, __arguments, generator) => {
  return new Promise((resolve, reject) => {
    var fulfilled = (value) => {
      try {
        step(generator.next(value));
      } catch (e) {
        reject(e);
      }
    };
    var rejected = (value) => {
      try {
        step(generator.throw(value));
      } catch (e) {
        reject(e);
      }
    };
    var step = (x) => x.done ? resolve(x.value) : Promise.resolve(x.value).then(fulfilled, rejected);
    step((generator = generator.apply(__this, __arguments)).next());
  });
};
var import_polkadot_launch = require("polkadot-launch");
var import_config = require("./config");
function shutdown() {
  (0, import_polkadot_launch.killAll)();
  process.exit(0);
}
function fail() {
  process.exit(2);
}
function main() {
  return __async(this, null, function* () {
    process.on("exit", shutdown).on("SIGINT", fail);
    yield (0, import_polkadot_launch.run)(__dirname, import_config.config);
  });
}
(() => __async(exports, null, function* () {
  return yield main();
}))();
