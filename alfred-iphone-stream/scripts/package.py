#!/usr/bin/env python3
from pathlib import Path
import plistlib
import sys
import uuid
import zipfile

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = "com.ariestwn.iphone-stream"

def uid(name):
    return str(uuid.uuid5(uuid.NAMESPACE_URL, BUNDLE + "/" + name)).upper()

def metadata():
    objects, connections, positions = [], {}, {}
    def add(name, kind, config, x, y, version=1):
        objects.append(dict(uid=uid(name), type="alfred.workflow." + kind, config=config, version=version))
        positions[uid(name)] = dict(xpos=x, ypos=y)
    def link(source, target):
        connections.setdefault(uid(source), []).append(dict(destinationuid=uid(target), modifiers=0, modifiersubtext="", vitoclose=True))
    add("menu", "input.scriptfilter", dict(keyword="{var:IPHONE_STREAM_KEYWORD}", title="iPhone Stream", subtext="Audio iPhone melalui USB · Stream, volume, Audio MIDI",
        script='exec ./iphone-stream filter -- "$1"', scriptfile="", scriptargtype=1, escaping=0, type=0,
        argumenttype=1, withspace=True, alfredfiltersresults=False, argumenttreatemptyqueryasnil=False,
        argumenttrimmode=0, queuedelaycustom=1, queuedelayimmediatelyinitially=True, queuedelaymode=0,
        queuemode=1, concurrently=False, runningsubtext="Memeriksa iPhone Stream…", skipuniversalaction=True), 40, 40, 3)
    link("menu", "run")
    commands = [
        ("toggle", "istream", "Toggle iPhone Stream"),
        ("volume-up", "ivup", "iPhone Volume Up"),
        ("volume-down", "ivdown", "iPhone Volume Down"),
        ("idam-toggle", "imidi", "Toggle iPhone Audio MIDI"),
    ]
    for index, (command, keyword, title) in enumerate(commands):
        y = 200 + index * 180
        add(command + "-keyword", "input.keyword", dict(keyword=keyword, text=title, subtext="iPhone Stream", argumenttype=2, withspace=False), 40, y)
        add(command + "-hotkey", "trigger.hotkey", dict(hotkey=0, hotmod=0, hotstring="", lefttext=title, righttext="", argument=0, focusedappvariable=False), 240, y)
        add(command + "-external", "trigger.external", dict(triggerid=command, availableviaurlhandler=True), 440, y)
        add(command + "-argument", "utility.argument", dict(argument=command, passthroughargument=False), 640, y)
        for source in [command + "-keyword", command + "-hotkey", command + "-external"]:
            link(source, command + "-argument")
        link(command + "-argument", "run")
    add("run", "action.script", dict(script='exec ./run-action "$1"', scriptfile="", scriptargtype=1, escaping=0, type=0, concurrently=False), 880, 100, 2)
    add("notify", "output.notification", dict(title="iPhone Stream", text="{query}", onlyshowifquerypopulated=True, lastpathcomponent=False, removeextension=False), 1100, 100)
    link("run", "notify")
    fields = [dict(variable="IPHONE_STREAM_KEYWORD", label="Keyword menu", description="Buka menu stream, volume, dan Audio MIDI.", type="textfield", config=dict(default="iphone", required=True, trim=True))]
    return dict(bundleid=BUNDLE, name="iPhone Stream", description="iPhone USB audio streaming controlled by Rust.", createdby="ariestwn", category="Music", version="0.1.0", disabled=False,
        objects=objects, connections=connections, uidata=positions, userconfigurationconfig=fields, variables={}, variablesdontexport=[], webaddress="", readme=(ROOT / "README.md").read_text())

def main():
    (ROOT / "workflow/info.plist").write_bytes(plistlib.dumps(metadata(), sort_keys=False))
    helper = ROOT / "workflow/iPhone Stream Helper.app/Contents"
    helper.mkdir(parents=True, exist_ok=True)
    (helper / "Info.plist").write_bytes(plistlib.dumps(dict(
        CFBundleIdentifier=BUNDLE + ".helper", CFBundleName="iPhone Stream Helper", CFBundleExecutable="iphone-stream-daemon",
        CFBundlePackageType="APPL", CFBundleVersion="1", CFBundleShortVersionString="0.1.0", LSMinimumSystemVersion="14.0", LSUIElement=True,
        NSMicrophoneUsageDescription="Play audio from your USB-connected iPhone through your Mac."), sort_keys=False))
    if "--metadata-only" in sys.argv:
        return
    out = ROOT / "dist/iPhone Stream.alfredworkflow"
    out.parent.mkdir(exist_ok=True)
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name in ["iphone-stream", "idam-helper", "run-action", "info.plist", "icon.png"]:
            archive.write(ROOT / "workflow" / name, name)
        for file in sorted((ROOT / "workflow/iPhone Stream Helper.app").rglob("*")):
            if file.is_file():
                archive.write(file, str(file.relative_to(ROOT / "workflow")))
        for name in ["README.md", "LICENSE", "THIRD_PARTY_NOTICES.md"]:
            archive.write(ROOT / name, name)
    print(out)

if __name__ == "__main__":
    main()
