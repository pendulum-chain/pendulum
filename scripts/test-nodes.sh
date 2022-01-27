#!/bin/bash

if [ $# -eq 0 ]
  then
    echo "\\nRun with \\n./test-nodes.sh <PARA_ID>\\n"
    exit 1
fi

PARA_ID=$1

rm -rf /tmp/parachain
rm -rf /tmp/relaychain
rm -rf ./logs

mkdir ./logs

# Start Relay `Alice` node
nohup ../polkadot/target/release/polkadot \
--alice \
--validator \
--base-path /tmp/relay/alice \
--chain ./specs/rococo-custom-2-raw.json \
--port 30333 \
--ws-port 9944 > logs/alice.log &

ALICE_PID=$!
echo "Alice running in pid $ALICE_PID"

# Start Relay `Bob` node
nohup ../polkadot/target/release/polkadot \
--bob \
--validator \
--base-path /tmp/relay-bob \
--chain "./specs/rococo-custom-2-raw.json" \
--port 30334 \
--ws-port 9945 > logs/bob.log &

BOB_PID=$!
echo "Bob running in pid $BOB_PID"

echo "Generating Parachain spec files"

# Generate plain spec file
./target/release/parachain-collator build-spec --disable-default-bootnode > "./specs/rococo-local-parachain-plain.json"

sed -E 's/"para_id": 2000/"para_id": '$PARA_ID'/' "./specs/rococo-local-parachain-plain.json" > "./specs/rococo-local-parachain-tmp-plain.json"
sed -E 's/"parachainId": 2000/"parachainId": '$PARA_ID'/' "./specs/rococo-local-parachain-tmp-plain.json" > "./specs/rococo-local-parachain-$PARA_ID-plain.json"
rm "./specs/rococo-local-parachain-tmp-plain.json"

./target/release/parachain-collator build-spec --chain "./specs/rococo-local-parachain-$PARA_ID-plain.json" --raw --disable-default-bootnode > "./specs/rococo-local-parachain-$PARA_ID-raw.json"
./target/release/parachain-collator export-genesis-wasm --chain "./specs/rococo-local-parachain-$PARA_ID-raw.json" > "./specs/para-$PARA_ID-wasm"
./target/release/parachain-collator export-genesis-state --chain "./specs/rococo-local-parachain-$PARA_ID-raw.json" > "./specs/para-$PARA_ID-genesis"

nohup ./target/release/parachain-collator \
--alice \
--collator \
--force-authoring \
--chain "./specs/rococo-local-parachain-$PARA_ID-raw.json" \
--enable-offchain-indexing "true" \
--base-path /tmp/parachain/alice \
--port 40333 \
--ws-port 8844 \
-- \
--execution wasm \
--chain "./specs/rococo-custom-2-raw.json" \
--port 30343 \
--ws-port 9977 > logs/collator.log &

COLLATOR_PID=$!
echo "Collator running in pid $COLLATOR_PID"

echo "Nodes are running."
echo "Check alice logs with tail -f logs/alice.log"
echo "Check bob logs with tail -f logs/bob.log"
echo "Check collator logs with tail -f logs/collator.log"
echo "Press Ctrl + C to finish all the processes at once"

wait $ALICE_PID
wait $BOB_PID
wait $COLLATOR_PID
