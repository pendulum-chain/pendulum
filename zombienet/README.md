**Zombienet for macos**

Download manualy from [zombienet](https://github.com/paritytech/zombienet/releases)

or using *wget* command:

`wget https://github.com/paritytech/zombienet/releases/download/v1.3.30/zombienet-macos`

- Give permission to execute *zombienet-macos* file

`chmod +x zombienet-macos`

- Check `--help`

`./zombienet-macos --help`


**Build Polkadot**

- build polkadot with *testnet* profile or production

`cargo build --profile testnet -p test-parachain-adder-collator` or `cargo build --release`

**Build pendulum-node**

```
cd pendulum/node
cargo build --release
```


- Specify chain in *config.toml*
  *pendulum* / *amplitude* / *foucoco*

  ```
  [[parachains]]
  ...
  chain = "foucoco" 
  ...
  ```
- Run zombienet (specify *provider* and path to zombienet *config.toml* file)

`./zombienet-macos spawn --provider native ./zombienet/config.toml`


Usefull link: 

**Parity zombienet** [repository](https://github.com/paritytech/zombienet)
