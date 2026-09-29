#!/usr/bin/env python3
"""Validate Alfred's graph, including the error route that must never paste."""
from pathlib import Path
import plistlib
import zipfile

root = Path(__file__).resolve().parent.parent
with zipfile.ZipFile(root / "dist/QuickAI.alfredworkflow") as archive:
    assert set(archive.namelist()) == {"info.plist", "quickai", "icon.png", "README.md", "LICENSE"}
    assert (archive.getinfo("quickai").external_attr >> 16) & 0o111
    workflow = plistlib.loads(archive.read("info.plist"))
    assert archive.read("quickai") == (root / "workflow/quickai").read_bytes()

objects = {obj["uid"]: obj for obj in workflow["objects"]}
assert len(objects) == len(workflow["objects"])
connections = workflow["connections"]
keywords = [obj["config"]["keyword"] for obj in objects.values() if obj["type"].endswith("input.keyword")]
assert set(keywords) == {"quickfix", "improve-writing", "grammar-check"} and len(keywords) == 3
assert len([o for o in objects.values() if o["type"].endswith("trigger.universalaction")]) == 3
assert len([o for o in objects.values() if o["type"].endswith("trigger.hotkey")]) == 3
for obj in objects.values():
    if obj['type'].endswith('output.dispatchkeycombo'):
        assert obj['config']['keycode'] == 8
        assert obj['config']['keymod'] == 1048576
for source, edges in connections.items():
    assert source in objects
    for edge in edges:
        assert edge["destinationuid"] in objects
        if "sourceoutputuid" in edge:
            assert edge["sourceoutputuid"] in {c["uid"] for c in objects[source]["config"]["conditions"]}

def path_for(ok, output, verify="0", ready="1", native="0"):
    script = next(o for o in objects.values() if o["type"].endswith("action.script") and '--workflow -- "$1"' in o["config"]["script"])
    current, visited = script["uid"], []
    variables = {"{var:quickai_ok}": ok, "{var:quickai_output}": output,
        "{var:quickai_verify}": verify, "{var:quickai_capture_ready}": ready,
        "{var:quickai_native_notification}": native}
    while current:
        obj = objects[current]
        visited.append(obj)
        edges = connections.get(current, [])
        if obj["type"].endswith("utility.conditional"):
            port = next((c["uid"] for c in obj["config"]["conditions"] if variables[c["inputstring"]] == c["matchstring"]), None)
            edges = [e for e in edges if e.get("sourceoutputuid") == port]
        assert len(edges) <= 1
        current = edges[0]["destinationuid"] if edges else None
    return visited

assert not any(o["type"].endswith("output.clipboard") for o in path_for("0", "paste"))
for ok in ["0", "1"]:
    for output in ["copy", "paste", "both"]:
        for native in ["0", "1"]:
            path = path_for(ok, output, native=native)
            feedback = [i for i, o in enumerate(path) if '--feedback' in o.get('config',{}).get('script','')]
            assert len(feedback) == 1
            assert all(i < feedback[0] for i, o in enumerate(path) if o['type'].endswith('output.clipboard'))
            assert any(o['type'].endswith('output.notification') for o in path) == (native == "1")
for output, autopaste, transient in [("copy", False, False), ("paste", True, True), ("both", True, False)]:
    clipboard = [o for o in path_for("1", output) if o["type"].endswith("output.clipboard")]
    assert len(clipboard) == 1
    assert clipboard[0]["config"]["autopaste"] == autopaste
    assert clipboard[0]["config"]["transient"] == transient
    assert clipboard[0]["config"]["ignoredynamicplaceholders"]
verified = path_for("1", "paste", "1")
assert any(o["type"].endswith("output.dispatchkeycombo") for o in verified)
assert any('--verify-paste' in o.get('config',{}).get('script','') for o in verified)
assert not any(o["type"].endswith("output.dispatchkeycombo") for o in path_for("1", "copy"))
for keyword in [o for o in objects.values() if o['type'].endswith('input.keyword')]:
    branch = objects[connections[keyword['uid']][0]['destinationuid']]
    assert branch['type'].endswith('utility.conditional')
    assert branch['config']['conditions'][0]['inputstring'] == '{query}'
    assert branch['config']['conditions'][0]['matchstring'] == ''
externals = [o for o in objects.values() if o["type"].endswith("trigger.external")]
assert {o["config"]["triggerid"] for o in externals} == set(keywords) and len(externals) == 3
for external in externals:
    assert external["config"]["availableviaurlhandler"]
    keyword = next(o for o in objects.values() if o["type"].endswith("input.keyword")
        and o["config"]["keyword"] == external["config"]["triggerid"])
    assert connections[external["uid"]] == connections[keyword["uid"]]
for field in workflow["userconfigurationconfig"]:
    if field["type"] == "popupbutton":
        assert field["config"]["default"] in [pair[1] for pair in field["config"]["pairs"]]
settings = {field["variable"]: field for field in workflow["userconfigurationconfig"]}
assert [pair[1] for pair in settings["QUICKAI_PROVIDER"]["config"]["pairs"]] == ["codex", "claude", "opencode"]
assert settings["QUICKAI_OPENCODE_MODEL"]["config"]["default"] == "deepseek/deepseek-flash"
assert "QUICKAI_OPENCODE_EFFORT" not in settings
assert settings["QUICKAI_OPENCODE_PATH"]["type"] == "textfield"
print("Verified archive, three commands, configuration defaults, and all output/error routes.")
