#!/usr/bin/env bash
# Resolves and verifies the release a packaging-channel workflow acts on.
#
# Inputs (environment):
#   EVENT_NAME        github.event_name
#   RELEASE_TAG       tag of the published release (release event)
#   DISPATCH_VERSION  x.y.z (workflow_dispatch)
#   GH_TOKEN, GH_REPO for the gh CLI
# Output: version= and tag= lines on $GITHUB_OUTPUT (stdout when unset).
#
# Accepts only stable x.y.z releases that are published, not drafts, not
# prereleases, and whose tag commit is an ancestor of origin/main.
set -euo pipefail

if [[ "${EVENT_NAME:-}" == workflow_dispatch ]]; then
  version="${DISPATCH_VERSION:?DISPATCH_VERSION is required}"
  tag="v${version}"
else
  tag="${RELEASE_TAG:?RELEASE_TAG is required}"
  version="${tag#v}"
fi

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "::error::Only stable x.y.z releases are packaged, got: ${version}"
  exit 1
fi

state="$(gh release view "$tag" --json isDraft,isPrerelease --jq '.isDraft,.isPrerelease')"
if [[ "$state" != $'false\nfalse' ]]; then
  echo "::error::${tag} must be a published, non-prerelease release"
  exit 1
fi

git fetch --quiet origin main --tags
if ! git show-ref --verify --quiet "refs/tags/${tag}"; then
  echo "::error::Release tag ${tag} does not exist"
  exit 1
fi
tag_sha="$(git rev-parse "refs/tags/${tag}^{commit}")"
if ! git merge-base --is-ancestor "$tag_sha" origin/main; then
  echo "::error::${tag} is not on main"
  exit 1
fi

{
  echo "version=${version}"
  echo "tag=${tag}"
} >> "${GITHUB_OUTPUT:-/dev/stdout}"
