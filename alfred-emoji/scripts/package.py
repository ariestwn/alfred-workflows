#!/usr/bin/env python3
from pathlib import Path
import plistlib
import uuid
import zipfile

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = "com.ariestwn.emoji"
FILES = ["emoji", "grid", "info.plist", "icon.png", "previous.png", "next.png"]
OUT = ROOT / "dist/Emoji.alfredworkflow"


def uid(name):
    return str(uuid.uuid5(uuid.NAMESPACE_URL, BUNDLE + "/" + name)).upper()


def metadata():
    objects, connections, positions = [], {}, {}

    def add(name, kind, config, x, y, version=1):
        objects.append(dict(uid=uid(name), type="alfred.workflow." + kind, config=config, version=version))
        positions[uid(name)] = dict(xpos=x, ypos=y)

    def link(source, target, modifier=0, title="", port=None):
        edge = dict(destinationuid=uid(target), modifiers=modifier, modifiersubtext=title, vitoclose=True)
        if port:
            edge["sourceoutputuid"] = uid(port)
        connections.setdefault(uid(source), []).append(edge)

    def clipboard(name, paste, y):
        add(name, "output.clipboard", dict(autopaste=paste, clipboardtext="{query}",
            ignoredynamicplaceholders=True, transient=False), 700, y, 3)

    add("keyword", "input.keyword", dict(keyword="{var:EMOJI_KEYWORD}", argumenttype=1,
        text="Search Emoji & Symbols", subtext="Type a word, feeling, or situation, then Return",
        withspace=True, skipuniversalaction=True), 40, 120)
    add("first-page", "utility.argument", dict(argument="{query}", passthroughargument=False,
        variables={"EMOJI_PAGE": "0"}), 200, 120)
    add("external", "trigger.external", dict(triggerid="browse", availableviaurlhandler=False), 200, 280)
    add("grid", "userinterface.grid", dict(columncount=8, filterable=False, fixedorder=True,
        imageaspect=0, inputfile="grid", inputtype=1, loadingtext="🔍 Looking for an emoji…",
        showsubtitles=False, showtitles=True, subtitlesinfooter=True, titlesinfooter=True), 380, 120)
    link("keyword", "first-page")
    link("first-page", "grid")
    link("external", "grid")
    add("kind", "utility.conditional", dict(conditions=[dict(inputstring="{var:EMOJI_KIND}",
        matchcasesensitive=True, matchmode=0, matchstring="page", outputlabel="Change page",
        uid=uid("kind-page"))], elselabel="Emoji", hideelse=False), 540, 20)
    add("reopen", "output.callexternaltrigger", dict(externaltriggerid="browse", passinputasargument=True,
        passvariables=True, workflowbundleid="self"), 700, 410)
    clipboard("paste", True, 20)
    clipboard("copy", False, 150)
    clipboard("copy-name", False, 280)
    link("grid", "kind")
    link("kind", "reopen", port="kind-page")
    link("kind", "paste")
    link("grid", "copy", 1048576, "Copy without pasting")
    link("grid", "copy-name", 524288, "Copy name")

    fields = [
        dict(variable="EMOJI_KEYWORD", label="Keyword", description="Opens the emoji grid.",
             type="textfield", config=dict(default="emoji", required=False, trim=True)),
        dict(variable="TYPESAFE_API_KEY", label="TypeSafe API key",
             description="Optional. Enables Jev semantic search for queries like “I got promoted”. Keyword search works without it.",
             type="textfield", config=dict(default="", required=False, trim=True, placeholder="ts_…")),
        dict(variable="JEV_MODEL", label="Jev model", description="TypeSafe System One model used for semantic search.",
             type="textfield", config=dict(default="jev-latest", required=False, trim=True)),
    ]
    return dict(bundleid=BUNDLE, name="Emoji", description="Emoji grid with keyword and Jev semantic search.",
                createdby="ariestwn", category="Productivity", version="0.1.0", disabled=False,
                objects=objects, connections=connections, uidata=positions, userconfigurationconfig=fields,
                variables={}, variablesdontexport=["TYPESAFE_API_KEY"], webaddress="",
                readme=(ROOT / "README.md").read_text())


def main():
    (ROOT / "workflow/info.plist").write_bytes(plistlib.dumps(metadata(), sort_keys=False))
    OUT.parent.mkdir(exist_ok=True)
    icons = sorted((ROOT / "workflow/icons").glob("*.png"))
    with zipfile.ZipFile(OUT, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name in FILES:
            archive.write(ROOT / "workflow" / name, name)
        for icon in icons:
            archive.write(icon, "icons/" + icon.name)
        for name in ["README.md", "LICENSE"]:
            archive.write(ROOT / name, name)
    print(f"{OUT} ({len(icons)} icons)")


if __name__ == "__main__":
    main()
