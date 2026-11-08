#!/usr/bin/env python3
"""Writes the two files that point Flatpak at Chilpa's own repository:
chilpa.flatpakrepo, which adds the repository as a remote, and
app.chilpa.Chilpa.flatpakref, which adds it and installs Chilpa in one step.
Both carry the repository's public key, so the commits and the summary signed
with it are verified."""
import argparse
import base64
from pathlib import Path
from urllib.parse import urlparse

APP_ID = "app.chilpa.Chilpa"
REMOTE = "chilpa"
TITLE = "Chilpa"
HOMEPAGE = "https://chilpa.app/"
RUNTIME_REPO = "https://dl.flathub.org/repo/flathub.flatpakrepo"


def repository_url(url):
    """The repository's address, which must be HTTPS and name a folder."""
    parsed = urlparse(url)
    if parsed.scheme != "https" or not parsed.netloc or parsed.query or parsed.fragment:
        raise ValueError(f"the repository URL must be a plain https:// address: {url!r}")
    if any(character.isspace() for character in url):
        raise ValueError(f"the repository URL holds whitespace: {url!r}")
    return url if url.endswith("/") else url + "/"


def public_key(data):
    """The key as the descriptors carry it: base64 on one line."""
    if not data:
        raise ValueError("the public key file is empty")
    if data.lstrip().startswith(b"-----BEGIN"):
        raise ValueError("pass the binary key (gpg --export), not an armored one")
    return base64.b64encode(data).decode("ascii")


def descriptors(url, key, branch, icon=None):
    if not branch or any(character in branch for character in "/ \n"):
        raise ValueError(f"not a branch name: {branch!r}")
    url, key = repository_url(url), public_key(key)
    icon_line = [f"Icon={icon}"] if icon else []
    repo = "\n".join(["[Flatpak Repo]", f"Title={TITLE}", f"Url={url}", f"Homepage={HOMEPAGE}",
                      f"DefaultBranch={branch}", *icon_line, f"GPGKey={key}"]) + "\n"
    ref = "\n".join(["[Flatpak Ref]", f"Title={TITLE}", f"Name={APP_ID}", f"Branch={branch}", f"Url={url}",
                     f"SuggestRemoteName={REMOTE}", f"Homepage={HOMEPAGE}", *icon_line,
                     f"RuntimeRepo={RUNTIME_REPO}", "IsRuntime=false", f"GPGKey={key}"]) + "\n"
    return {f"{REMOTE}.flatpakrepo": repo, f"{APP_ID}.flatpakref": ref}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", required=True, help="https:// address of the OSTree repository")
    parser.add_argument("--key", required=True, type=Path, help="public key, as gpg --export writes it")
    parser.add_argument("--branch", default="stable")
    parser.add_argument("--icon", help="https:// address of an icon for software centers")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for name, text in descriptors(args.url, args.key.read_bytes(), args.branch, args.icon).items():
        (args.output / name).write_text(text)
        print(args.output / name)


if __name__ == "__main__":
    main()
