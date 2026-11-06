#!/usr/bin/env bash
# REL-2: bump reproducible. Uso: make bump [TYPE=patch|minor|major | VERSION=X.Y.Z]
# Sin TYPE ni VERSION, el tipo se infiere de los commits desde el último tag (REL-1).
# Crea el commit `chore(release): vX.Y.Z` y el tag anotado; no hace push.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

RELEASE_BRANCH=main
CHECK_CMD=${CHECK_CMD:-make check}
TYPE=${TYPE:-}
VERSION=${VERSION:-}
SEMVER='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-rc\.[0-9]+)?$'

die() { echo "bump: $*" >&2; exit 1; }

[[ -z $TYPE || -z $VERSION ]] || die "usa TYPE o VERSION, no ambos"
[[ -z $TYPE || $TYPE =~ ^(patch|minor|major)$ ]] || die "TYPE inválido: $TYPE (patch|minor|major)"
[[ -z $VERSION || $VERSION =~ $SEMVER ]] || die "VERSION inválida: $VERSION"

branch=$(git branch --show-current)
[[ $branch == "$RELEASE_BRANCH" ]] || die "ejecuta el bump en '$RELEASE_BRANCH' (rama actual: '${branch:-detached}')"
[[ -z $(git status --porcelain) ]] || die "el árbol de trabajo no está limpio"

pkg=$(awk -F'"' '/^\[package\]/{p=1; next} /^\[/{p=0} p && /^name = /{print $2; exit}' Cargo.toml)
current=$(awk -F'"' '/^\[package\]/{p=1; next} /^\[/{p=0} p && /^version = /{print $2; exit}' Cargo.toml)
[[ -n $pkg ]] || die "no se encontró el nombre del paquete en Cargo.toml"
[[ $current =~ $SEMVER ]] || die "versión actual no es SemVer: '$current'"

last_tag=$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true)
range=${last_tag:+$last_tag..}HEAD
subjects=$(git log --no-merges --format='%s' "$range")
bodies=$(git log --no-merges --format='%B' "$range")

if [[ -z $VERSION ]]; then
    if [[ -z $TYPE ]]; then
        if grep -qE '^[a-z]+(\([^)]*\))?!:' <<<"$subjects" || grep -q 'BREAKING CHANGE' <<<"$bodies"; then
            TYPE=major
        elif grep -qE '^feat(\([^)]*\))?:' <<<"$subjects"; then
            TYPE=minor
        else
            TYPE=patch
        fi
        echo "bump: tipo inferido desde ${last_tag:-el inicio}: $TYPE"
    fi
    IFS=. read -r ma mi pa <<<"${current%%-*}"
    case $TYPE in
        major) VERSION="$((ma + 1)).0.0" ;;
        minor) VERSION="$ma.$((mi + 1)).0" ;;
        patch) VERSION="$ma.$mi.$((pa + 1))" ;;
    esac
fi

newest=$(printf '%s\n%s\n' "$current" "$VERSION" | sort -V | tail -1)
[[ $VERSION != "$current" && $newest == "$VERSION" ]] || die "la versión $VERSION debe ser mayor que $current"

tag="v$VERSION"
git rev-parse -q --verify "refs/tags/$tag" >/dev/null && die "el tag $tag ya existe localmente"

git fetch -q origin "$RELEASE_BRANCH" || die "no se pudo consultar el remoto"
[[ -z $(git ls-remote --tags origin "refs/tags/$tag") ]] || die "el tag $tag ya existe en el remoto"
[[ $(git rev-parse HEAD) == $(git rev-parse "origin/$RELEASE_BRANCH") ]] \
    || die "'$RELEASE_BRANCH' no coincide con origin/$RELEASE_BRANCH"

echo "bump: ejecutando '$CHECK_CMD'"
$CHECK_CMD || die "'$CHECK_CMD' falló"

# Escribe en temporales y reemplaza al final; ante cualquier fallo, restaura.
files=(Cargo.toml Cargo.lock CHANGELOG.md)
tmp=$(mktemp -d)
restore() {
    for f in "${files[@]}"; do
        if git cat-file -e "HEAD:$f" 2>/dev/null; then
            git checkout -q HEAD -- "$f"
        else
            git rm -q --cached --ignore-unmatch -- "$f"
            rm -f "$f"
        fi
    done
    rm -rf "$tmp"
}
trap 'restore; echo "bump: abortado, sin cambios" >&2' ERR

awk -v v="$VERSION" '
    /^\[package\]/ {p=1} /^\[/ && !/^\[package\]/ {p=0}
    p && !done && /^version = / {print "version = \"" v "\""; done=1; next} {print}
' Cargo.toml >"$tmp/Cargo.toml"

awk -v n="$pkg" -v v="$VERSION" '
    $0 == "name = \"" n "\"" {print; hit=1; next}
    hit && /^version = / {print "version = \"" v "\""; hit=0; next} {hit=0; print}
' Cargo.lock >"$tmp/Cargo.lock"

section() {
    local title=$1 pattern=$2 lines
    lines=$(grep -E "$pattern" <<<"$subjects" | sed -E 's/^[a-z]+(\([^)]*\))?!?: */- /' || true)
    [[ -z $lines ]] || printf '### %s\n\n%s\n\n' "$title" "$lines"
}
notes=$(
    section Added '^feat(\([^)]*\))?!?:'
    section Fixed '^fix(\([^)]*\))?!?:'
    section Changed '^(perf|refactor)(\([^)]*\))?!?:'
)
[[ -n $notes ]] || notes="### Changed

- Mantenimiento interno sin cambios funcionales."

existing=$(cat CHANGELOG.md 2>/dev/null || printf '# Changelog\n')
header=$(awk '/^## /{exit} {print}' <<<"$existing")
previous=$(awk '/^## /{f=1} f' <<<"$existing")
{
    printf '%s\n\n## [%s] - %s\n\n%s\n' "$header" "$VERSION" "$(date +%F)" "$notes"
    [[ -z $previous ]] || printf '\n%s\n' "$previous"
} >"$tmp/CHANGELOG.md"

for f in "${files[@]}"; do mv "$tmp/$f" "$f"; done
grep -q "version = \"$VERSION\"" Cargo.toml || false
grep -A1 "^name = \"$pkg\"" Cargo.lock | grep -q "version = \"$VERSION\"" || false

git add -- "${files[@]}"
git commit -q -m "chore(release): $tag"
trap - ERR
rm -rf "$tmp"

git tag -a "$tag" -m "Release $tag"
echo "bump: $current -> $VERSION (commit y tag $tag creados; sin push)"
echo "bump: publica con: git push origin $RELEASE_BRANCH && git push origin $tag"
