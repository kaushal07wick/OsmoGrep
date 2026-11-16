#!/bin/sh
set -eu
cat > /app/maths.py <<'PY'
def mean(values):
    if not values:
        raise ValueError('empty values')
    return sum(values) / len(values)
PY
