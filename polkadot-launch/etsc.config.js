const esbuildPluginTsc = require("esbuild-plugin-tsc");

module.exports = {
    outDir: "./build",
    esbuild: {
        entryPoints: ["./src/index.ts"],
        outdir: "./build",
        tsconfig: "./tsconfig.json",
        plugins: [esbuildPluginTsc()],
    },
};
