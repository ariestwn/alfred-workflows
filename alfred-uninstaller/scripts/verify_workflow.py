#!/usr/bin/env python3
"""Verify the archive, acyclic Alfred graph, and the packaged Rust executable."""
from pathlib import Path
import json
import os
import plistlib
import stat
import subprocess
import tempfile
import zipfile
from package import ROOT, BUNDLE, FILES, uid

def main():
    archive = ROOT/"dist/Uninstaller Rust.alfredworkflow"
    with zipfile.ZipFile(archive) as z, tempfile.TemporaryDirectory(prefix="uninstaller-package-test-") as tmp:
        names = set(z.namelist())
        assert set(FILES) <= names
        assert all(not Path(n).is_absolute() and ".." not in Path(n).parts for n in names)
        assert all(n in FILES or n in ["README.md","LICENSE","THIRD_PARTY_NOTICES.md"] or n.startswith("licenses/") for n in names)
        assert z.getinfo("uninstaller").external_attr >> 16 & stat.S_IXUSR
        graph = plistlib.loads(z.read("info.plist"))
        assert graph["bundleid"] == BUNDLE
        objects = {o["uid"]:o for o in graph["objects"]}
        assert len(objects) == len(graph["objects"])
        edges = graph["connections"]
        seen, active = set(), set()
        def visit(source):
            assert source in objects
            assert source not in active, "Alfred object graphs must be acyclic"
            if source in seen: return
            active.add(source)
            for edge in edges.get(source,[]):
                if "sourceoutputuid" in edge:
                    assert edge["sourceoutputuid"] in {c["uid"] for c in objects[source]["config"]["conditions"]}
                visit(edge["destinationuid"])
            active.remove(source); seen.add(source)
        for source in objects: visit(source)
        assert objects[uid("reopen")]["config"]["externaltriggerid"] == objects[uid("review-trigger")]["config"]["triggerid"]
        assert objects[uid("review")]["config"]["keyword"] == ""
        review_routes = {edge["modifiers"]:edge["destinationuid"] for edge in edges[uid("review")]}
        assert review_routes == {0:uid("route"),524288:uid("route"),1048576:uid("edit"),262144:uid("edit")}
        assert objects[uid("edit")]["config"]["script"] == 'exec ./uninstaller edit "$1"'
        def finder_target(item, modifier=None):
            selected = item if modifier is None else item["mods"][modifier]
            assert selected["valid"] and selected["arg"]
            route = selected["variables"]["UNINSTALL_ROUTE"]
            condition = next(c for c in objects[uid("route")]["config"]["conditions"] if c["matchstring"] == route)
            assert condition["inputstring"] == "{var:UNINSTALL_ROUTE}"
            edge = next(e for e in edges[uid("route")] if e.get("sourceoutputuid") == condition["uid"])
            assert edge["vitoclose"] is False
            target = objects[edge["destinationuid"]]
            assert edge["destinationuid"] not in edges  # A Finder action cannot loop into removal.
            return target
        for obj in objects.values():
            script = obj["config"].get("script", "")
            assert "{query}" not in script
            if script: assert script.endswith('"$1"')
        z.extractall(tmp)
        binary = Path(tmp)/"uninstaller"; binary.chmod(0o755)
        version = json.loads(subprocess.check_output([str(binary),"--version"]))
        assert version["version"] == graph["version"]
        # A deliberately unique app fixture keeps this verification read-only on user apps.
        root = Path(tmp)/"Applications"; root.mkdir()
        app = root/"Alfred Uninstaller Verification.app"
        (app/"Contents/MacOS").mkdir(parents=True)
        (app/"Contents/MacOS/main").write_text("test executable")
        (app/"Contents/Info.plist").write_bytes(plistlib.dumps(dict(CFBundleIdentifier="com.example.alfred-uninstaller-verification",CFBundleName="Alfred Uninstaller Verification",CFBundleExecutable="main",CFBundlePackageType="APPL")))
        fake_brew = Path(tmp)/"brew"
        # A shell script, not Python: under Rosetta, an Intel build cannot launch arm64-only Python.
        fake_brew.write_text('''#!/bin/sh
root=$(cd "$(dirname "$0")" && pwd -P)
if [ "$*" = "info --json=v2 --cask --installed" ]; then
    cat "$root/brew-info.json"
elif [ "$*" = "--caskroom" ]; then
    printf '%s\\n' "$root"
else
    echo "Unexpected Homebrew mutation during package verification" >&2
    exit 1
fi
''')
        fake_brew.chmod(0o755)
        (Path(tmp)/"brew-info.json").write_text(json.dumps(dict(casks=[dict(token="verification-cask", installed="1.0", artifacts=[dict(app=[app.name],target=str(app))])])) )
        env = dict(os.environ, UNINSTALL_APP_FOLDERS=str(root), UNINSTALL_BREW_PATH=str(fake_brew), alfred_workflow_cache=str(Path(tmp)/"cache"))
        def run(command,arg="",extra=None):
            return json.loads(subprocess.check_output([str(binary),command,arg],env=dict(env,**(extra or {}))))
        listing = run("list","Alfred Uninstaller Verification")
        item = next(i for i in listing["items"] if i.get("valid"))
        started = run("action",item["arg"])
        variables = started["alfredworkflow"]["variables"]
        assert variables["UNINSTALL_SESSION"] and not variables["UNINSTALL_ERROR"]
        review = run("view",extra=variables)
        assert review["items"][0]["title"] == "Uninstall Alfred Uninstaller Verification · Homebrew"
        assert json.loads(review["items"][0]["arg"])["op"] == "uninstall"
        assert len(review["items"]) == 2 and "1 of 1" in review["items"][0]["subtitle"]
        assert review["items"][1]["title"].startswith("☑ ")
        assert review["items"][1]["arg"] == review["items"][0]["arg"]
        rejected = run("edit",review["items"][0]["arg"])["alfredworkflow"]
        assert rejected["variables"]["UNINSTALL_ERROR"] and app.exists()
        toggled = run("edit",review["items"][1]["mods"]["cmd"]["arg"])["alfredworkflow"]
        unchecked = run("view",toggled["arg"],toggled["variables"])
        assert unchecked["items"][1]["title"].startswith("☐ ")
        assert unchecked["items"][0]["valid"] is False
        assert unchecked["items"][1]["valid"] is False
        assert unchecked["items"][1]["mods"]["cmd"]["valid"] is True
        opened = run("edit",unchecked["items"][0]["mods"]["ctrl"]["arg"])["alfredworkflow"]
        menu = run("view",opened["arg"],opened["variables"])
        assert menu["items"][0]["title"] == "Back to files"
        selected = run("action",menu["items"][1]["arg"])["alfredworkflow"]
        restored = run("view",selected["arg"],selected["variables"])
        assert restored["items"][1]["title"].startswith("☑ ")
        assert restored["items"][0]["valid"] is True
        # Never execute default Return here: removal is covered by isolated fake-Trash/cask tests.
        assert app.exists()
        # Simulate a completed move in the fixture tree to verify the actual packaged result UI.
        destination = Path(tmp)/"Mock Trash"/"Verification Café 2.app"
        destination.parent.mkdir()
        app.rename(destination)
        session_path = Path(env["alfred_workflow_cache"])/"reviews"/(variables["UNINSTALL_SESSION"] + ".json")
        session = json.loads(session_path.read_text())
        session["phase"] = "Finished"
        candidate = session["scan"]["candidates"][0]
        candidate.update(trashed_to=str(destination), result="Moved to Trash by Finder")
        session_path.write_text(json.dumps(session))
        report = run("view",extra=variables)
        summary, file = report["items"]
        for modifier in [None, "alt"]:
            target = finder_target(summary, modifier)
            assert target["type"] == "alfred.workflow.action.openfile"
            assert target["config"] == dict(sourcefile="", openwith="/System/Library/CoreServices/Finder.app")
            selected_summary = summary if modifier is None else summary["mods"][modifier]
            assert Path(selected_summary["arg"]) == Path.home()/".Trash"
            target = finder_target(file, modifier)
            assert target["type"] == "alfred.workflow.action.revealfile"
            assert target["config"] == dict(path="")  # Native action consumes the selected argument.
            selected_file = file if modifier is None else file["mods"][modifier]
            assert selected_file["arg"] == str(destination)
        assert destination.exists()
    print("Verified archive, graph, packaged selection and Actions, and Return/Option-Return result routes to Finder")

if __name__ == "__main__": main()
