**Zombienet for macos**

Download from [zombienet](https://github.com/paritytech/zombienet/releases)

or

`wget https://github.com/paritytech/zombienet/releases/download/v1.3.30/zombienet-macos`

Give permission to execute zombienet-macos file

`chmod +x zombienet-macos`

Check --help to any option.

`./zombienet-macos --help`

Build Polkadot

`cargo build --release`

Build Foucoco

`cargo build --release`

Run zombienet

`./zombienet-macos spawn --provider native config.toml`
