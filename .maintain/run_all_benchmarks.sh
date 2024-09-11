#!/bin/bash

script_dir=$(dirname "$(realpath "$0")")


runtimes=("pendulum" "amplitude" "foucoco")


for runtime_name in "${runtimes[@]}"; do
  cd $script_dir
  cd ../runtime/$runtime_name/src/weights
  weight_dir=`pwd`

  cd $script_dir

  if [ ! -d "$weight_dir" ]; then
    echo "Directory $weight_dir does not exist for runtime $runtime_name!"
    echo "Weights directory should exist and contain previously calculated weights"
    continue
  fi

  echo "Processing runtime: $runtime_name"

  for file in "$weight_dir"/*; do

    filename=$(basename -- "$file")
    filename_without_ext="${filename%.*}"


    if [[ "$filename_without_ext" == "parachain_staking" ]]; then
      echo "Skipping file: $filename_without_ext"
      continue
    fi

    echo "Running benchmark for pallet: $filename_without_ext in runtime $runtime_name"

    # Run the benchmark command for each file, ignore errors from files
    # not corresponding to any pallets.
    # Failed benchmarks will not be detected.
    ../target/production/pendulum-node benchmark pallet \
      --chain $runtime_name \
      --wasm-execution=compiled \
      --pallet "$filename_without_ext" \
      --extrinsic "*" \
      --steps 50 \
      --repeat 20 \
      --output "../runtime/$runtime_name/src/weights/$filename" \
      --template "frame-weight-template.hbs" \
      || true

  done
done
