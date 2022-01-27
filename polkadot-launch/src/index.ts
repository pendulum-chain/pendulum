import { killAll, run } from "polkadot-launch";
import { config } from "./config"

function shutdown() {
    killAll();
    process.exit(0);
}

function fail() {
    process.exit(2)
}

async function main() {
    process
        .on("exit", shutdown)
        .on("SIGINT", fail);

    await run(__dirname, config);
}

(async () => await main())();
