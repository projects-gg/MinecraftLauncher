#!/usr/bin/env bash
# Installs Ubuntu packages on a GitHub runner without hanging on a stuck mirror (1 Nov 2026: two
# runs waited 40 and 48 minutes in `apt-get update` on azure.archive.ubuntu.com). Each try has a
# time limit and a stalled connection fails in 30 seconds; after a failure or a timeout there is
# one more try, without the Azure mirror when the runner's mirror list has another one.
# Usage: apt-install.sh <package>...
set -uo pipefail

mirrors=/etc/apt/apt-mirrors.txt
opts=(-o Acquire::Retries=2 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30)

attempt() {
  sudo timeout -k 10 120 apt-get update "${opts[@]}" &&
    sudo timeout -k 10 300 apt-get install -y "${opts[@]}" "$@"
}

if attempt "$@"; then
  exit 0
fi
echo "::warning::apt-get failed or took too long; one more try"
if [ -f "$mirrors" ] && grep -v azure "$mirrors" | grep -q ubuntu; then
  sudo sed -i '/azure\.archive\.ubuntu\.com/d' "$mirrors"
fi
# An install cut off while dpkg ran leaves it half done.
sudo dpkg --configure -a || true
attempt "$@"
