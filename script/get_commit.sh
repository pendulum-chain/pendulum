#!/bin/bash
set -e

versionNumber=$1

tagNamePrefix="node-release"
## get the latest tag of this node
lastVersionName=$(git describe --abbrev=0 --tags --always `git rev-list --tags` | grep -i "$tagNamePrefix" -m 1)

## get the commits since the last tag
latestCommitOfLastVersion=$(git log $lastVersionName --oneline --max-count=1 | cut -c1-7)


logs=( $(git log $latestCommitOfLastVersion..origin/main --oneline -- node/ | cut -c1-7) )
 if (( ${#logs[@]} == 0 )); then
     echo "Error: Repo is up to date. No new release required".
     exit 1
 fi

echo -e "## What's Changed\n" >> Commits.txt
## output the relevant commits, and save to file
echo "relevant commits:"
for commit in  "${logs[@]}"; do
  link="$(git log --format="[%h](https://github.com/pendulum-chain/pendulum/commit/%h) %s" -n 1 $commit)"
  echo $link
  echo -e "* "$link"\n" >> Commits.txt
done

$newVersionName
echo "new version: "$tagNamePrefix"-"$versionNumber
