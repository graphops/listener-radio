#!/usr/bin/env bash
set -e
set -x

VERSION="$(cargo metadata --quiet --format-version 1 | jq -r '.packages[] | select(.name == "graphcast-3la") | .version')"

if [[ -z "$VERSION" ]]; then
  echo "Usage: $0 <version>"
  exit 1
fi

git-cliff -o CHANGELOG.md

(
  git add CHANGELOG.md Cargo.lock Cargo.toml &&
    git commit -m "chore: release $VERSION"
) || true

cargo publish
