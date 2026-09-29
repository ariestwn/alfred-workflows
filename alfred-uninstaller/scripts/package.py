#!/usr/bin/env python3
"""Build a clean workflow graph and importable archive; no personal data is exported."""
from pathlib import Path
import plistlib
import uuid
import zipfile

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = "com.ariestwn.uninstaller-rust"
NAME = "Uninstaller Rust"
FILES = ["info.plist", "uninstaller", "icon.png", "checked.png", "unchecked.png", "trash.png", "back.png", "warning.png"]

def uid(name):
    return str(uuid.uuid5(uuid.NAMESPACE_URL, BUNDLE + "/" + name)).upper()

def metadata():
    objects, connections, positions = [], {}, {}
    def add(name, kind, config, x, y, version=1):
        objects.append(dict(uid=uid(name), type="alfred.workflow." + kind, config=config, version=version))
        positions[uid(name)] = dict(xpos=x, ypos=y)
    def link(source, target, port=None, keep_open=True, modifiers=0):
        edge = dict(destinationuid=uid(target), modifiers=modifiers, modifiersubtext="", vitoclose=keep_open)
        if port: edge["sourceoutputuid"] = uid(port)
        connections.setdefault(uid(source), []).append(edge)
    for name, keyword, command, title, description, x, y in [
        ("apps", "{var:UNINSTALL_KEYWORD}", "list", "Uninstall Applications", "Find an app to uninstall", 40, 60),
        ("review", "", "view", "Uninstall Application", "Return to uninstall · ⌘Return to select · ⌃Return for actions", 340, 440),
    ]:
        add(name, "input.scriptfilter", dict(keyword=keyword, title=title, subtext=description,
            argumenttype=1, argumenttreatemptyqueryasnil=False, argumenttrimmode=0, withspace=True,
            alfredfiltersresults=False, alfredfiltersresultsmatchmode=0, skipuniversalaction=True,
            queuedelaycustom=1, queuedelayimmediatelyinitially=True, queuedelaymode=0, queuemode=1,
            runningsubtext="Finding applications…" if name == "apps" else "Loading files…",
            script=f'exec ./uninstaller {command} "$1"', scriptargtype=1, scriptfile="", escaping=0, type=0), x, y, 3)
        link(name, "route")
        # Alfred needs modifier connections as well as JSON mods to use their arguments.
        link(name, "route", modifiers=524288)  # Option: reveal
        if name == "review":
            link(name, "edit", modifiers=1048576)  # Command: selection only
            link(name, "edit", modifiers=262144)  # Control: Actions only
    add("route", "utility.conditional", dict(conditions=[dict(inputstring="{var:UNINSTALL_ROUTE}",
        matchcasesensitive=True, matchmode=0, matchstring=route, outputlabel=label, uid=uid("route-" + route))
        for route, label in [("reveal", "Reveal in Finder"), ("trash", "Open Trash")]],
        elselabel="Review action", hideelse=False), 640, 160)
    link("route", "action")
    link("route", "finder", "route-reveal", keep_open=False)
    add("finder", "action.revealfile", dict(path=""), 920, 50)
    link("route", "trash", "route-trash", keep_open=False)
    add("trash", "action.openfile", dict(sourcefile="", openwith="/System/Library/CoreServices/Finder.app"), 1200, 50)
    for name, command, x, y in [("action", "action", 920, 240), ("edit", "edit", 640, 500), ("start", "start", 340, 700)]:
        add(name, "action.script", dict(concurrently=False, escaping=0, script=f'exec ./uninstaller {command} "$1"',
            scriptargtype=1, scriptfile="", type=0), x, y, 2)
        link(name, "reopen")
    add("reopen", "output.callexternaltrigger", dict(externaltriggerid="review", passinputasargument=True,
        passvariables=True, workflowbundleid="self"), 1200, 400)
    add("review-trigger", "trigger.external", dict(triggerid="review", availableviaurlhandler=False), 40, 440)
    link("review-trigger", "review")
    add("universal", "trigger.universalaction", dict(acceptsfiles=True, acceptsmulti=0, acceptstext=False,
        acceptsurls=False, name="Uninstall Application…"), 40, 660)
    add("external", "trigger.external", dict(triggerid="uninstall_app", availableviaurlhandler=False), 40, 820)
    link("universal", "start"); link("external", "start")
    return dict(bundleid=BUNDLE, name=NAME, description="Uninstall apps through Homebrew when managed, then move selected leftovers to Trash.",
        createdby="ariestwn", category="Tools", version="0.4.1", disabled=False, objects=objects,
        connections=connections, uidata=positions, variables={}, variablesdontexport=[], webaddress="",
        readme=(ROOT / "README.md").read_text(), userconfigurationconfig=[
            dict(variable="UNINSTALL_KEYWORD", label="Keyword", type="textfield", description="Find apps to uninstall.",
                config=dict(default="uninstall", trim=True, required=False)),
            dict(variable="UNINSTALL_APP_FOLDERS", label="Additional application folders", type="textarea",
                description="One absolute or ~/ folder path per line. /Applications and ~/Applications are always included. Nested app bundles and symlinks are excluded.",
                config=dict(default="", trim=True, required=False, verticalsize=4)),
            dict(variable="UNINSTALL_BREW_PATH", label="Homebrew executable (optional)", type="textfield",
                description="Leave blank to detect Apple Silicon, Intel, and PATH installations. Set an absolute path to use one particular Homebrew installation.",
                config=dict(default="", trim=True, required=False)),
        ])

def main():
    (ROOT / "workflow/info.plist").write_bytes(plistlib.dumps(metadata(), sort_keys=False))
    output = ROOT / "dist" / (NAME + ".alfredworkflow")
    output.parent.mkdir(exist_ok=True)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name in FILES: archive.write(ROOT / "workflow" / name, name)
        for name in ["README.md", "LICENSE", "THIRD_PARTY_NOTICES.md"]: archive.write(ROOT / name, name)
        for path in sorted((ROOT / "licenses").iterdir()): archive.write(path, "licenses/" + path.name)
    print(output)

if __name__ == "__main__": main()
