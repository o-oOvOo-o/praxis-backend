#!/bin/bash

# Set "chatgpt.cliExecutable": "/Users/<USERNAME>/code/praxis/scripts/debug-praxis.sh" in VSCode settings to always get the 
# latest praxis-rs binary when debugging Praxis.


set -euo pipefail

PRAXIS_RS_DIR=$(realpath "$(dirname "$0")/../praxis-rs")
(cd "$PRAXIS_RS_DIR" && cargo run --quiet --bin praxis -- "$@")