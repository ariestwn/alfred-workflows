#!/usr/bin/env python3
"""Check release archives: one CPU architecture per build, and no secrets or local settings."""
from pathlib import Path
import plistlib
import re
import subprocess
import sys
import tempfile
import zipfile

MACHO = {b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe"}
EXPECTED = {"apple-silicon": "arm64", "intel": "x86_64"}
FORBIDDEN_NAMES = re.compile(r"(^|/)(prefs\.plist|\.env(\..*)?|\.DS_Store|__pycache__/.*|.*\.pyc)$")
SECRET = re.compile(
    rb"(?<![A-Za-z0-9-])sk-(?:proj-|ant-api\d+-|svcacct-)?(?=[A-Za-z0-9_-]*\d)[A-Za-z0-9_-]{20,}"
    rb"|gh[pous]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,}"
    rb"|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{35}|xox[abpr]-[A-Za-z0-9-]{10,}"
    rb"|-----BEGIN [A-Z ]*PRIVATE KEY-----"
)


def lipo_archs(data):
    with tempfile.NamedTemporaryFile() as tmp:
        tmp.write(data)
        tmp.flush()
        return subprocess.check_output(["/usr/bin/lipo", "-archs", tmp.name], text=True).split()


def check(path):
    label = next((l for l in EXPECTED if path.stem.endswith("-" + l)), None)
    binaries = []
    with zipfile.ZipFile(path) as archive:
        info = plistlib.loads(archive.read("info.plist"))
        assert info.get("variables", {}) == {}, f"{path.name}: info.plist exports workflow variables"
        for entry in archive.infolist():
            if entry.is_dir():
                continue
            assert not FORBIDDEN_NAMES.search(entry.filename), f"{path.name}: unexpected file {entry.filename}"
            data = archive.read(entry)
            match = SECRET.search(data)
            assert not match, f"{path.name}: {entry.filename} contains a secret-like value"
            if data[:4] in MACHO:
                archs = lipo_archs(data)
                assert label, f"{path.name}: {entry.filename} is native code in an architecture-neutral build"
                assert archs == [EXPECTED[label]], f"{path.name}: {entry.filename} is {archs}"
                assert (entry.external_attr >> 16) & 0o111, f"{path.name}: {entry.filename} is not executable"
                binaries.append(entry.filename)
    assert binaries or not label, f"{path.name}: no native binary found"
    return label or "any Mac", binaries


def main():
    folder = Path(sys.argv[1] if len(sys.argv) > 1 else "release")
    archives = sorted(folder.glob("*.alfredworkflow"))
    assert archives, f"No archives in {folder}"
    for path in archives:
        label, binaries = check(path)
        print(f"ok  {path.name}  ({label}; {len(binaries)} native binaries)")
    print(f"Checked {len(archives)} archives.")


if __name__ == "__main__":
    main()
