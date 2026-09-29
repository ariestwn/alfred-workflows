#!/usr/bin/env python3
"""Validate the archive and trace success/error routes through Alfred's graph."""
from pathlib import Path
import plistlib
import zipfile

ROOT = Path(__file__).resolve().parent.parent
with zipfile.ZipFile(ROOT / "dist/Upload to Remote.alfredworkflow") as archive:
    assert set(archive.namelist()) == {"info.plist", "kraely-upload", "icon.png", "README.md", "LICENSE"}
    assert (archive.getinfo("kraely-upload").external_attr >> 16) & 0o111
    assert archive.read("kraely-upload") == (ROOT / "workflow/kraely-upload").read_bytes()
    workflow = plistlib.loads(archive.read("info.plist"))

objects = {obj["uid"]: obj for obj in workflow["objects"]}
connections = workflow["connections"]
assert len(objects) == len(workflow["objects"])
triggers = [o for o in objects.values() if ".trigger." in o["type"]]
assert len(triggers) == 2
universal = next(o for o in triggers if o["type"] == "alfred.workflow.trigger.universalaction")
assert universal["config"] == dict(acceptsfiles=True, acceptsmulti=False,
    acceptstext=False, acceptsurls=False, name="Upload to Remote")
external = next(o for o in triggers if o["type"] == "alfred.workflow.trigger.external")
assert external["config"] == dict(availableviaurlhandler=False, triggerid="upload")
upload_uid = connections[universal["uid"]][0]["destinationuid"]
assert [e["destinationuid"] for e in connections[external["uid"]]] == [upload_uid]
scripts = [o for o in objects.values() if o["type"].endswith("action.script")]
assert any(o["config"]["script"] == 'exec ./kraely-upload --workflow -- "$@"' for o in scripts)
assert all(o["config"]["scriptargtype"] == 1 for o in scripts)
for source, edges in connections.items():
    assert source in objects
    for edge in edges:
        assert edge["destinationuid"] in objects
        if "sourceoutputuid" in edge:
            assert edge["sourceoutputuid"] in {c["uid"] for c in objects[source]["config"]["conditions"]}


def route(trigger, ok, native):
    variables = {"{var:kraely_ok}": ok, "{var:kraely_native_notification}": native}
    current, visited = trigger["uid"], []
    while current:
        assert current not in [o["uid"] for o in visited], "Graph cycle"
        obj = objects[current]
        visited.append(obj)
        edges = connections.get(current, [])
        if obj["type"].endswith("utility.conditional"):
            port = next((c["uid"] for c in obj["config"]["conditions"] if variables[c["inputstring"]] == c["matchstring"]), None)
            edges = [e for e in edges if e.get("sourceoutputuid") == port]
        assert len(edges) <= 1
        current = edges[0]["destinationuid"] if edges else None
    return visited


for trigger, ok, native in [(t, o, n) for t in triggers for o in "01" for n in "01"]:
    path = route(trigger, ok, native)
    copies = [i for i, o in enumerate(path) if o["type"].endswith("output.clipboard")]
    feedback = [i for i, o in enumerate(path) if "--feedback" in o["config"].get("script", "")]
    assert len(copies) == int(ok)
    assert len(feedback) == 1
    assert all(i < feedback[0] for i in copies), "Success feedback must follow clipboard copy"
    assert any(o["type"].endswith("output.notification") for o in path) == (native == "1")
    for i in copies:
        assert path[i]["config"] == dict(autopaste=False, clipboardtext="{query}",
            ignoredynamicplaceholders=True, transient=False)

fields = workflow["userconfigurationconfig"]
assert isinstance(fields, list)
assert {f["variable"] for f in fields} == {"KRAELY_HOST", "KRAELY_REMOTE_DIR", "KRAELY_MAX_SIZE_MB", "KRAELY_TIMEOUT", "KRAELY_FEEDBACK"}
host = next(f for f in fields if f["variable"] == "KRAELY_HOST")
assert host["type"] == "textfield"
assert host["config"]["default"] == ""
for field in fields:
    if field["type"] == "popupbutton":
        assert field["config"]["default"] in [pair[1] for pair in field["config"]["pairs"]]
print("Verified archive, Universal Action and External Trigger, configuration, and success/error clipboard routes.")
