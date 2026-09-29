#!/usr/bin/env python3
"""Install a separate workflow, or explicitly update only this workflow's packaged files."""
from pathlib import Path
import plistlib
import shutil
import sys
import tempfile
import uuid
import zipfile
from package import BUNDLE, FILES, ROOT

workflow_root = Path(sys.argv[1]).expanduser().resolve(strict=True)
destination = workflow_root / ("user.workflow." + str(uuid.uuid5(uuid.NAMESPACE_URL, BUNDLE)).upper())
update = "--update" in sys.argv[2:]
if destination.exists():
    if not update or plistlib.loads((destination / "info.plist").read_bytes())["bundleid"] != BUNDLE:
        raise SystemExit("Already installed: " + str(destination))
with zipfile.ZipFile(ROOT / "dist/AI Chat.alfredworkflow") as archive:
    if set(archive.namelist()) != set(FILES + ["README.md", "LICENSE"]):
        raise SystemExit("Unexpected archive contents")
    if plistlib.loads(archive.read("info.plist"))["bundleid"] != BUNDLE:
        raise SystemExit("Unexpected workflow identity")
    stage = Path(tempfile.mkdtemp(prefix=".ai-chat-install-", dir=workflow_root))
    try:
        for name in archive.namelist():
            (stage / name).write_bytes(archive.read(name))
            (stage / name).chmod(0o755 if name in ("ai-chat", "chat-view") else 0o644)
        if destination.exists():
            for path in stage.iterdir():
                path.replace(destination / path.name)
        else:
            stage.rename(destination)
    finally:
        if stage.exists():
            shutil.rmtree(stage)
print(destination)
