#!/usr/bin/env python3
"""Bundle resolved dependency notices and a version inventory."""
from pathlib import Path
import json
import os
import platform
import subprocess

root = Path(__file__).resolve().parent.parent
target = os.environ.get("TARGET") or ("aarch64-apple-darwin" if platform.machine() == "arm64" else "x86_64-apple-darwin")
metadata = json.loads(subprocess.check_output([str(root/"scripts/cargo.sh"), "metadata", "--locked", "--offline", "--format-version", "1", "--filter-platform", target], cwd=root))
resolved = {n["id"] for n in metadata["resolve"]["nodes"]}
folder = root/"licenses"
folder.mkdir(exist_ok=True)
inventory = ["# Third-party notices", "", "The Rust dependency license texts are included in `licenses/`.", "", "| Crate | Version | License |", "| --- | --- | --- |"]
for package in sorted(metadata["packages"], key=lambda p: p["name"]):
    if package["name"] == "alfred-uninstaller" or package["id"] not in resolved: continue
    inventory.append(f'| {package["name"]} | {package["version"]} | {package["license"] or "See bundled notice"} |')
    source = Path(package["manifest_path"]).parent
    for file in source.iterdir():
        if file.is_file() and file.name.upper().startswith(("LICENSE", "COPYING", "NOTICE")):
            (folder/f'{package["name"]}-{package["version"]}-{file.name}').write_bytes(file.read_bytes())
(root/"THIRD_PARTY_NOTICES.md").write_text("\n".join(inventory)+"\n")
print(f"Bundled {len(list(folder.iterdir()))} dependency notices")
