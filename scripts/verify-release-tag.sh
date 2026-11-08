#!/usr/bin/env bash
# verify-release-tag.sh [--on-main] <tag>
# In a checkout with its history and tags: the tag is a release tag and main
# already holds its commit. Without --on-main the checkout must also be the
# tag's own commit. With --on-main the checkout is main's, the trusted code a
# job holding a secret runs, and the tag's commit is printed on standard
# output, alone, for that job to compare.
set -euo pipefail

on_main=false
if [[ "${1:-}" == --on-main ]]; then on_main=true; shift; fi
tag="${1:?tag}"
if [[ ! "$tag" =~ ^v[0-9] ]]; then
  echo "::error::Not a release tag: ${tag}" >&2
  exit 1
fi
if ! tagged="$(git rev-parse --verify --quiet "refs/tags/${tag}^{commit}")"; then
  echo "::error::The tag ${tag} is not in this checkout" >&2
  exit 1
fi
if [[ "$on_main" == false && "$tagged" != "$(git rev-parse HEAD)" ]]; then
  echo "::error::The checkout is not ${tag}'s commit" >&2
  exit 1
fi
if ! git merge-base --is-ancestor "$tagged" origin/main; then
  echo "::error::Only commits that main already holds are released; promote them to main before tagging." >&2
  exit 1
fi
if [[ "$on_main" == true ]]; then
  echo "$tagged"
else
  echo "${tag} is ${tagged}, on main"
fi
