#!/bin/sh
set -eu
workspace=$(CDPATH= cd -P "$(dirname "$0")" && pwd)
exec python3 "$workspace/music-link/start.py" "$@"
