#!/bin/sh
set -eu
workspace=$(CDPATH= cd -P "$(dirname "$0")" && pwd)
exec sh "$workspace/Start Music.sh" "$@"
