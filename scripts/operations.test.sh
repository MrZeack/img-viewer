#!/usr/bin/env bash
# MK-11: contrato del Makefile y scripts operativos.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

fail=0
err() { echo "FAIL: $*" >&2; fail=1; }

[[ -f Makefile ]] || { echo "FAIL: falta Makefile" >&2; exit 1; }

targets=$({ make -qp 2>/dev/null || true; } | awk -F: '/^[a-zA-Z][a-zA-Z0-9_-]*:/ {print $1}' | sort -u)
for t in install check build bump; do
    grep -qx "$t" <<<"$targets" || err "falta el target '$t'"
done

if grep -nE 'pkill|killall|lsof -ti|fuser -k' Makefile scripts/*.sh | grep -v 'operations.test.sh'; then
    err "comandos de parada globales prohibidos (MK-2)"
fi

for s in scripts/*.sh; do
    bash -n "$s" || err "$s no pasa bash -n"
done

[[ ! -e build.sh ]] || err "build.sh debe reemplazarse por el Makefile"

if [[ -e .env ]] && ! git check-ignore -q .env; then
    err ".env no está ignorado"
fi

[[ $fail -eq 0 ]] && echo "operations: ok"
exit $fail
