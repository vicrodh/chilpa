#!/usr/bin/env bash
# verify-release-tag.sh <tag>
# In a checkout with its history and tags: the tag is a release tag, the
# checkout is the tag's own commit, and main already holds that commit.
# Releases come only from main; a job that holds a signing key runs this
# first.
set -euo pipefail

tag="${1:?tag}"
if [[ ! "$tag" =~ ^v[0-9] ]]; then
  echo "::error::Not a release tag: ${tag}"
  exit 1
fi
if ! tagged="$(git rev-parse --verify --quiet "refs/tags/${tag}^{commit}")"; then
  echo "::error::The tag ${tag} is not in this checkout"
  exit 1
fi
if [[ "$tagged" != "$(git rev-parse HEAD)" ]]; then
  echo "::error::The checkout is not ${tag}'s commit"
  exit 1
fi
if ! git merge-base --is-ancestor HEAD origin/main; then
  echo "::error::Only commits that main already holds are released; promote them to main before tagging."
  exit 1
fi
echo "${tag} is ${tagged}, on main"
