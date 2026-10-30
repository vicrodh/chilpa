#!/usr/bin/env bash
# test-pkgs-repo.sh [version] [workdir]
#
# End-to-end check of the APT and DNF repositories against a PUBLISHED release:
# builds them with a throwaway GPG key, serves them through the real Worker
# source (node), and installs chilpa from them in ubuntu:24.04, debian:13 and
# fedora containers. Each distro also proves that a tampered index is rejected.
# Needs docker and node. Containers use --network host and reach the local
# server on 127.0.0.1; package downloads follow the Worker's 302 to GitHub.
set -euo pipefail

version="${1:-0.1.0}"
work="${2:-$(mktemp -d)}"
here="$(cd "$(dirname "$0")" && pwd)"
port=18080; bad_port=18081
mkdir -p "$work"
echo "workdir: $work"
results=()
pids=()
cleanup() { for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT

# 1. The published packages, verified against SHA256SUMS.
"$here/fetch-assets.sh" "v${version}" "$work/dl" \
  "chilpa_${version}_amd64.deb" "chilpa_${version}_arm64.deb" \
  "chilpa-${version}-1.x86_64.rpm" "chilpa-${version}-1.aarch64.rpm"
mkdir -p "$work/packages"
mv "$work/dl"/chilpa* "$work/packages/"

# 2. Build with a throwaway key, inside a container that has the tools.
rm -rf "$work/site"; mkdir -p "$work/site" "$work/gnupg"; chmod 700 "$work/gnupg"
docker run --rm -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" -e BASE_URL="http://127.0.0.1:${port}" \
  -v "$here:/scripts:ro" -v "$work/packages:/packages:ro" -v "$work/site:/site" -v "$work/gnupg:/gnupg" \
  ubuntu:24.04 bash -euc '
    apt-get update -qq && apt-get install -y -qq --no-install-recommends apt-utils createrepo-c gnupg xz-utils gzip >/dev/null
    export GNUPGHOME=/gnupg
    gpg --batch --passphrase "" --quick-generate-key "Chilpa test <test@invalid>" rsa2048 sign never 2>/dev/null
    /scripts/build-pkgs-repo.sh /packages /site
    chown -R "$HOST_UID:$HOST_GID" /site /gnupg
  '
find "$work/site" -type f | sort

# 3. A tampered copy: the Release text and repomd.xml are altered after signing.
rm -rf "$work/site-bad"; cp -a "$work/site" "$work/site-bad"
sed -i 's/^Suite: stable/Suite: evil/' "$work/site-bad/apt/dists/stable/InRelease" "$work/site-bad/apt/dists/stable/Release"
for a in x86_64 aarch64; do echo "<!-- tampered -->" >> "$work/site-bad/dnf/$a/repodata/repomd.xml"; done

# 4. Serve both through the real Worker.
node "$here/pkgs-local-server.mjs" "$work/site" "$port" 2> "$work/server.log" & pids+=($!)
node "$here/pkgs-local-server.mjs" "$work/site-bad" "$bad_port" 2> "$work/server-bad.log" & pids+=($!)
for _ in $(seq 1 20); do curl -fs "http://127.0.0.1:${port}/chilpa.asc" >/dev/null && break; sleep 0.5; done

record() { results+=("$1: $2"); }

apt_case() { # apt_case <image>
  local image="$1" log="$work/${1//[:\/]/_}.log"
  if docker run --rm --network host ubuntu:24.04 true >/dev/null 2>&1 && docker run --rm --network host \
      -e PORT="$port" -e BAD="$bad_port" -e VERSION="$version" "$image" bash -euc '
        apt-get update -qq && apt-get install -y -qq --no-install-recommends ca-certificates curl gnupg >/dev/null
        curl -fsS "http://127.0.0.1:$PORT/chilpa.gpg" -o /usr/share/keyrings/chilpa.gpg
        # Tampered index first: apt-get update must refuse it.
        echo "deb [signed-by=/usr/share/keyrings/chilpa.gpg] http://127.0.0.1:$BAD/apt stable main" > /etc/apt/sources.list.d/chilpa.list
        if apt-get update 2>&1 | tee /tmp/bad.log | grep -qE "(not signed|BAD signature|is not signed|could not be verified|invalid)"; then
          echo "TAMPER-REJECTED"
        else
          cat /tmp/bad.log; echo "TAMPER-ACCEPTED"; exit 1
        fi
        echo "deb [signed-by=/usr/share/keyrings/chilpa.gpg] http://127.0.0.1:$PORT/apt stable main" > /etc/apt/sources.list.d/chilpa.list
        apt-get update 2>&1 | grep -E "chilpa|Err|W:" || true
        apt-cache policy chilpa | head -4
        DEBIAN_FRONTEND=noninteractive apt-get install -y chilpa 2>&1 | tail -5
        dpkg -s chilpa | grep -E "^(Status|Version)"
        test -x /opt/chilpa/bin/chilpa-qt -o -x /usr/bin/chilpa-qt -o -n "$(dpkg -L chilpa | grep chilpa-qt | head -1)"
        echo "INSTALL-OK"
      ' > "$log" 2>&1; then
    record "$image" PASS
  else
    record "$image" "FAIL (see $log)"
  fi
  grep -E "TAMPER|INSTALL-OK|^Status|^Version" "$log" || tail -15 "$log"
}

apt_case ubuntu:24.04
apt_case debian:13

# Fedora: repo_gpgcheck=1 must reject the tampered repomd.xml, then install.
flog="$work/fedora.log"
if docker run --rm --network host -e PORT="$port" -e BAD="$bad_port" fedora:latest bash -euc '
      curl -fsS "http://127.0.0.1:$PORT/chilpa.repo" | sed "s#\$basearch#x86_64#" > /tmp/good.repo
      sed "s#127.0.0.1:$PORT#127.0.0.1:$BAD#" /tmp/good.repo > /tmp/bad.repo
      cp /tmp/bad.repo /etc/yum.repos.d/chilpa.repo
      if dnf -y --disablerepo="*" --enablerepo=chilpa makecache 2>&1 | tee /tmp/bad.log | grep -qiE "(signature|gpg|verif|failed)"; then
        echo "TAMPER-REJECTED"
      else
        cat /tmp/bad.log; echo "TAMPER-ACCEPTED"; exit 1
      fi
      cp /tmp/good.repo /etc/yum.repos.d/chilpa.repo
      dnf -y --refresh install chilpa 2>&1 | tail -8
      rpm -q chilpa
      echo "INSTALL-OK"
    ' > "$flog" 2>&1; then
  record fedora:latest PASS
else
  record fedora:latest "FAIL (see $flog)"
fi
grep -E "TAMPER|INSTALL-OK|^chilpa" "$flog" || tail -15 "$flog"

echo; echo "== results (amd64/x86_64 only; arm64 packages are indexed but not installed) =="
printf '%s\n' "${results[@]}"
printf '%s\n' "${results[@]}" | grep -q FAIL && exit 1 || exit 0
