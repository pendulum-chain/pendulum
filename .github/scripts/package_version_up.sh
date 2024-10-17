#!/bin/bash
set -e

# define the version number we want to set
newVersion=$1

# Find all Cargo.toml files in the current directory and its subdirectories
for file in $(find . -name "Cargo.toml")
do
    # Use awk to change the version number of the package
    awk -v newVersion="$newVersion" -F'=' '/\[package\]/,/version =/ { if($0 ~ /version =/ && $0 !~ /#/) {print $1 "= ""\""newVersion"\""; next} }1' $file > temp && mv temp $file
done