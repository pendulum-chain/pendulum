#!/bin/bash

# It saves into a filename: commits_for_<runtime>_release-release-<number>
# all the commits relevant to the given runtime, from the last release up to
# the latest commit of the main branch of Pendulum repo/
# scripts$ ./get_commits.sh <runtime>


chosenRuntime="$1"
## forcefully set to lowercase
chosenRuntime=$( echo "$chosenRuntime" |  tr -s  '[:upper:]'  '[:lower:]')


## list of runtimes available
runtimesList=("foucoco" "amplitude" "pendulum")

excludeRuntimes=''
## will make comparison case-insensitive
for runtime in "${runtimesList[@]}"; do
  ## exclude runtimes that is NOT chosen
  if [[ "$runtime" != "$chosenRuntime" ]]; then
    if [ -z "${excludeRuntimes}" ]; then
      excludeRuntimes=$runtime
    else
      excludeRuntimes="$excludeRuntimes|$runtime"
    fi
  fi
done

if [ -z "${excludeRuntimes}" ]; then
  echo "unsupported runtime "$chosenRuntime
  exit
fi

## get the latest tag of this runtime
lastVersionName=$(git describe --abbrev=0 --tags --always `git rev-list --tags` | grep -i "$chosenRuntime*" -m 1)
echo "last version: "$lastVersionName

## extract the version number of the version
lastVersionNumber=$(echo $lastVersionName | sed 's/[^0-9]*//g')
newVersionName=$chosenRuntime-release-$((lastVersionNumber+1))
echo "new version:  "$newVersionName
fileName="commits_for_"$newVersionName

## remove the file if existed
if test -f $fileName; then
  rm $fileName
fi

## get the commits since the last tag
latestCommitOfLastVersion=$(git log $lastVersionName --oneline --max-count=1 | cut -c1-7)

if [ -z "${latestCommitOfLastVersion}" ]; then
  echo "last version is up to date."
  exit
fi

## only list commits related to this runtime and the general code changes
## save to file commits_for_<chosen_runtime>-release-<new version number>.txt
git log $latestCommitOfLastVersion..origin --oneline | grep -i -Ev "$excludeRuntimes" |cut -c1-7 >> $fileName


