#!/usr/bin/env bash
# System packages cox-voice (T54.2, A123) builds against: cmake for
# whisper.cpp, libclang for whisper-rs-sys' bindgen, and on Linux the ALSA
# headers cpal links. cox-voice is a workspace member and `cox`'s `voice`
# feature pulls it in, so every `--workspace` or `--all-features` build
# needs them. macOS runner images ship cmake, and bindgen finds libclang
# through Xcode, so there is nothing to install there. Run by the `rust`
# job's setup-command in ci.yml and by sonarcloud.yml.
set -euo pipefail
if [ "$(uname -s)" != Linux ]; then exit 0; fi
sudo apt-get update
sudo apt-get install -y --no-install-recommends cmake libclang-dev libasound2-dev pkg-config
