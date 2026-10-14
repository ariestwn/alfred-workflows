#!/usr/bin/env python3
from pathlib import Path
import plistlib
import stat
import subprocess
import zipfile

root = Path(__file__).resolve().parent.parent
with zipfile.ZipFile(root / "dist/iPhone Stream.alfredworkflow") as archive:
    names = set(archive.namelist())
    binaries = ["iphone-stream", "idam-helper", "iPhone Stream Helper.app/Contents/MacOS/iphone-stream-daemon"]
    for name in binaries + ["run-action"]:
        assert name in names, name
        assert (archive.getinfo(name).external_attr >> 16) & stat.S_IXUSR, name
    plist = plistlib.loads(archive.read("info.plist"))
    ids = {obj["uid"] for obj in plist["objects"]}
    for source, edges in plist["connections"].items():
        assert source in ids
        assert all(edge["destinationuid"] in ids for edge in edges)
    assert len([obj for obj in plist["objects"] if obj["type"] == "alfred.workflow.trigger.external"]) == 4
    assert "NSMicrophoneUsageDescription" in plistlib.loads(archive.read("iPhone Stream Helper.app/Contents/Info.plist"))
    assert not any(name.endswith(("state.json", "daemon.pid", "daemon.log")) for name in names)
architectures = [subprocess.check_output(["/usr/bin/lipo", "-archs", str(root / "workflow" / name)], text=True).strip() for name in binaries]
assert len(set(architectures)) == 1, architectures
print("Workflow verified: connections, executable permissions, microphone metadata, matching architectures.")
