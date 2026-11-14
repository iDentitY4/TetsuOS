#!/bin/sh
# Buildroot's host packages on the release workflow's Ubuntu runners
# (docs/developer/building.md lists them for a PC).
set -eu
sudo apt-get update -q
sudo apt-get install -y -q --no-install-recommends \
	build-essential bc bison flex cpio rsync unzip wget curl file perl python3 \
	patch bzip2 git libncurses-dev e2fsprogs openssl
