#!/bin/bash
set -e

runtime=$1
versionName=$2

if [ $runtime == "amplitude" ]; then
  file="runtime/amplitude/src/lib.rs"
elif [ $runtime == "pendulum" ]; then
  file="runtime/pendulum/src/lib.rs"
fi

echo "increment: $versionName"
# Use awk to increment the version of the lib
awk -v pat="$versionName" -F':' '$0~pat { {print $1": " $2+1","; next} }1' $file > temp && mv temp $file



