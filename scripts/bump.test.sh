#!/usr/bin/env bash
# REL-2: make bump falla sin tocar archivos ante precondiciones inválidas
# y, en el caso válido, crea solo el commit de release y el tag anotado.
set -euo pipefail
# Corre dentro de `make bump TYPE=… | VERSION=…`: no heredar los parámetros del bump real.
unset TYPE VERSION CHECK_CMD

ROOT=$(git rev-parse --show-toplevel)
BUMP="$ROOT/scripts/bump.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

fail=0
ok() { echo "  ok: $*"; }
err() { echo "  FAIL: $*" >&2; fail=1; }

# Repositorio temporal con remoto propio, a partir del HEAD actual.
setup() {
    rm -rf "$TMP/remote.git" "$TMP/work"
    git clone -q --bare "$ROOT" "$TMP/remote.git"
    git clone -q "$TMP/remote.git" "$TMP/work"
    cd "$TMP/work"
    git config user.name test
    git config user.email test@example.invalid
    git config tag.gpgSign false
    git config commit.gpgSign false
    git checkout -q main 2>/dev/null || git checkout -q -b main
    [[ -f CHANGELOG.md ]] || printf '# Changelog\n' >CHANGELOG.md
    git add -A && git commit -q --allow-empty -m "feat: synthetic feature"
    git push -q origin HEAD:main
}

current_version() { awk -F'"' '/^version = /{print $2; exit}' Cargo.toml; }

expect_failure() {
    local name=$1; shift
    local before after
    before=$(git status --porcelain; git rev-parse HEAD; cat Cargo.toml Cargo.lock CHANGELOG.md | shasum)
    if env CHECK_CMD=true "$@" "$BUMP" >/dev/null 2>&1; then
        err "$name: debía fallar"
    else
        after=$(git status --porcelain; git rev-parse HEAD; cat Cargo.toml Cargo.lock CHANGELOG.md | shasum)
        [[ "$before" == "$after" ]] && ok "$name" || err "$name: modificó archivos"
    fi
}

echo "bump:"

setup
echo "dirty" >>README.md
expect_failure "árbol sucio" TYPE=patch
git checkout -q -- README.md

expect_failure "tipo inválido" TYPE=huge
expect_failure "versión inválida" VERSION=1.2
expect_failure "versión no mayor" VERSION=0.0.1
git tag -a v8.8.8 -m local
expect_failure "tag existente local" VERSION=8.8.8
git tag -d v8.8.8 >/dev/null
git --git-dir="$TMP/remote.git" tag v9.9.9 main
expect_failure "tag existente en remoto" VERSION=9.9.9

git checkout -q -b other
expect_failure "fuera de main" TYPE=patch
git checkout -q main

git commit -q --allow-empty -m "fix: local only"
expect_failure "main no coincide con el remoto" TYPE=patch
git reset -q --hard origin/main

expect_failure "check en rojo" TYPE=patch CHECK_CMD=false

# Caso válido: inferencia por commits (feat -> minor).
old=$(current_version)
IFS=. read -r ma mi _ <<<"$old"
new="$ma.$((mi + 1)).0"
if CHECK_CMD=true "$BUMP" >/dev/null 2>&1; then
    [[ $(current_version) == "$new" ]] && ok "Cargo.toml en $new" || err "Cargo.toml: $(current_version)"
    grep -A1 '^name = "img_viewer"' Cargo.lock | grep -q "version = \"$new\"" \
        && ok "Cargo.lock sincronizado" || err "Cargo.lock no sincronizado"
    grep -q "^## \[$new\] - " CHANGELOG.md && ok "sección en CHANGELOG" || err "falta sección en CHANGELOG"
    grep -q "synthetic feature" CHANGELOG.md && ok "changelog desde commits" || err "changelog sin commits"
    [[ $(git log -1 --format=%s) == "chore(release): v$new" ]] && ok "mensaje de commit" || err "mensaje: $(git log -1 --format=%s)"
    files=$(git show --name-only --format= HEAD | LC_ALL=C sort | tr '\n' ' ')
    [[ "$files" == "CHANGELOG.md Cargo.lock Cargo.toml " ]] && ok "solo manifiestos y changelog" || err "archivos: $files"
    [[ $(git cat-file -t "v$new") == tag ]] && ok "tag anotado" || err "tag no anotado"
    [[ -z $(git status --porcelain) ]] && ok "árbol limpio" || err "árbol sucio tras bump"
    [[ -z $(git ls-remote --tags origin "v$new") ]] && ok "sin push" || err "hizo push del tag"
else
    err "bump válido falló"
fi

exit $fail
