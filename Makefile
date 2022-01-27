.SILENT: all clean run build-parachain Build-launcher clean-parachain clean-launcher 

all: build-parachain build-launcher

clean: clean-parachain clean-launcher

run:
	@if [[ ! -f ./target/release/parachain-collator ]]; then make build-launcher; fi
	@if [[ ! -f ./polkadot-launch/build/index.js ]]; then make build-launcher; fi
	@cd ./polkadot-launch && yarn start

build-parachain:
	@echo "Building parachain..."
	@cargo build --release

build-launcher:
	@echo "Building launcher..."
	@cd ./polkadot-launch && yarn install && yarn build

clean-parachain:
	# read -r -p "Are You Sure? [Y/n] " input
	# case $$input in \
    	# [yY]|[yY][eE][sS]) \
	# 		echo "Removing parachain binaries..." \
	# 		;; \
	# 	[nN]|[oO][nN]) \
	# 		echo "Aborting..." \
	# 		;; \
    	# *) \
	# 		echo "Invalid input..." ;; \
	# esac
	@echo "Removing parachain binaries..."
	@cargo clean 

clean-launcher:
	@echo "Cleaning launcher..."
	@cd ./polkadot-launch && yarn clean

