#!/bin/sh
set -eu
python3 /fixture/provider.py &
exec "$@"
