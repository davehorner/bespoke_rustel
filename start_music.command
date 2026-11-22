#!/bin/sh
set -eu
workspace=$(CDPATH= cd -P "$(dirname "$0")" && pwd)
exec sh "$workspace/start_music.sh" "$@"
