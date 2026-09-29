#!/usr/bin/env python3
"""Bundle the resolved dependency copyright/license notices."""
from pathlib import Path
import json
import os
import platform
import subprocess

root=Path(__file__).resolve().parent.parent
target=os.environ.get("TARGET") or ("aarch64-apple-darwin" if platform.machine()=="arm64" else "x86_64-apple-darwin")
metadata=json.loads(subprocess.check_output([str(root/"scripts/cargo.sh"),"metadata","--locked","--offline","--format-version","1","--filter-platform",target],cwd=root))
resolved={n['id'] for n in metadata['resolve']['nodes']}
folder=root/"licenses"
folder.mkdir(exist_ok=True)
count=0
for package in metadata['packages']:
    if package['name']=='alfred-calculator' or package['id'] not in resolved: continue
    source=Path(package['manifest_path']).parent
    for file in source.iterdir():
        if file.is_file() and file.name.upper().startswith(('LICENSE','COPYING','NOTICE')):
            (folder/f"{package['name']}-{package['version']}-{file.name}").write_bytes(file.read_bytes())
            count+=1
print(f"Bundled {count} dependency notices")
