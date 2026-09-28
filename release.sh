#!/usr/bin/env bash
# Cut a release: bump Cargo.toml's version, commit everything, tag it, push.
# Pushing a tag shaped like a version (see .github/workflows/release.yml) is what
# kicks off the actual GitHub release build — so this is the point of no easy return.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

branch=$(git rev-parse --abbrev-ref HEAD)
if [[ "$branch" != "main" ]]; then
    echo "On branch '$branch', not main — releases go out from main. Aborting." >&2
    exit 1
fi

current=$(grep -m1 '^version = ' Cargo.toml | sed -E 's/version = "(.*)"/\1/')
IFS='.' read -r major minor patch <<< "$current"

read -rp "Commit message: " message
if [[ -z "$message" ]]; then
    echo "A commit message is required." >&2
    exit 1
fi

read -rp "Version bump (major/minor/patch): " bump
bump=$(printf '%s' "$bump" | tr '[:upper:]' '[:lower:]')
case "$bump" in
    major) major=$((major + 1)); minor=0; patch=0 ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    patch) patch=$((patch + 1)) ;;
    *) echo "Bump must be major, minor or patch, not '$bump'." >&2; exit 1 ;;
esac

new="$major.$minor.$patch"
tag="v$new"

if git rev-parse "$tag" >/dev/null 2>&1; then
    echo "Tag $tag already exists." >&2
    exit 1
fi

echo "$current -> $new"
sed -i.bak -E "s/^version = \".*\"/version = \"$new\"/" Cargo.toml
rm -f Cargo.toml.bak

# Cargo.lock carries this crate's own version too, and cargo only rewrites it on a
# real build — skip this and the lock silently drifts from Cargo.toml (see 8222519).
cargo build

git add -A
git commit -m "$message"
git tag "$tag"
git push
git push origin "$tag"

echo "Released $tag"
