#!/usr/bin/env python3
"""Install the checksum-pinned Schema Advisor binary for this app release."""

import hashlib
import json
import os
from pathlib import Path
import platform
import re
import sys
import tempfile
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, build_opener


ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    ("Darwin", "arm64"): "aarch64-apple-darwin",
    ("Linux", "x86_64"): "x86_64-unknown-linux-gnu",
}
RELEASE_HOSTS = {"github.com", "release-assets.githubusercontent.com"}


class ReleaseRedirect(HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, new_url):
        target = urlsplit(new_url)
        if target.scheme != "https" or target.netloc not in RELEASE_HOSTS:
            raise ValueError("GitHub redirected the release download to an unexpected host.")
        return super().redirect_request(request, fp, code, message, headers, new_url)


def install():
    release = json.loads((ROOT / "release.json").read_text(encoding="utf-8"))
    manifest = json.loads((ROOT / "lastdb-app.json").read_text(encoding="utf-8"))
    version = release["version"]
    if version != manifest["version"] or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise ValueError("The pinned binary version must match the app manifest.")
    target = TARGETS.get((platform.system(), platform.machine()))
    if not target:
        raise ValueError("Schema Advisor supports Apple Silicon macOS and Linux x86-64.")
    checksum = release["sha256"][target]
    if not re.fullmatch(r"[0-9a-f]{64}", checksum):
        raise ValueError("The binary pin must be a lowercase SHA-256 digest.")

    url = f"https://github.com/EdgeVector/schema-advisor/releases/download/v{version}/schema-advisor-{target}"
    destination = ROOT / "dist" / "schema-advisor"
    destination.parent.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="download-", dir=destination.parent) as directory:
        temporary = Path(directory) / "schema-advisor"
        digest = hashlib.sha256()
        size = 0
        opener = build_opener(ReleaseRedirect)
        with opener.open(url, timeout=25) as response, temporary.open("wb") as output:
            for chunk in iter(lambda: response.read(65536), b""):
                size += len(chunk)
                if size > 32 * 1024 * 1024:
                    raise ValueError("The advisor release exceeds the 32 MiB download limit.")
                digest.update(chunk)
                output.write(chunk)
        if digest.hexdigest() != checksum:
            raise ValueError("The downloaded advisor does not match the pinned SHA-256 digest.")
        temporary.chmod(0o755)
        os.replace(temporary, destination)
    print(f"Installed Schema Advisor {version} for {target}.")


if __name__ == "__main__":
    try:
        install()
    except (KeyError, OSError, ValueError) as error:
        sys.exit(f"Schema Advisor install failed: {error}")
