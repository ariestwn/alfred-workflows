#!/usr/bin/env python3
"""Generate the Universal Action and package the standalone Rust executable."""
from pathlib import Path
import plistlib
import uuid
import zipfile

ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = ROOT / "workflow"
BUNDLE = "com.ariestwn.kraely-upload"


def uid(name):
    return str(uuid.uuid5(uuid.NAMESPACE_URL, BUNDLE + "/" + name)).upper()


def metadata():
    objects, connections, positions = [], {}, {}

    def add(name, kind, config, x, y, version=1):
        objects.append(dict(uid=uid(name), type="alfred.workflow." + kind, config=config, version=version))
        positions[uid(name)] = dict(xpos=x, ypos=y)

    def link(source, dest, port=None):
        edge = dict(destinationuid=uid(dest), modifiers=0, modifiersubtext="", vitoclose=False)
        if port:
            edge["sourceoutputuid"] = uid(port)
        connections.setdefault(uid(source), []).append(edge)

    def condition(name, variable, label, otherwise, x, y):
        add(name, "utility.conditional", dict(conditions=[dict(inputstring=variable,
            matchcasesensitive=True, matchmode=0, matchstring="1", outputlabel=label,
            uid=uid(name + "-yes"))], elselabel=otherwise, hideelse=False), x, y)

    def script(name, command, x, y):
        add(name, "action.script", dict(concurrently=False, escaping=0,
            script=command, scriptargtype=1, scriptfile="", type=0), x, y, 2)

    add("universal", "trigger.universalaction", dict(acceptsfiles=True, acceptsmulti=False,
        acceptstext=False, acceptsurls=False, name="Upload to Remote"), 40, 100)
    add("external", "trigger.external", dict(availableviaurlhandler=False,
        triggerid="upload"), 40, 220)
    script("upload", 'exec ./kraely-upload --workflow -- "$@"', 250, 100)
    condition("success", "{var:kraely_ok}", "Uploaded", "Upload failed", 460, 100)
    add("copy", "output.clipboard", dict(autopaste=False, clipboardtext="{query}",
        ignoredynamicplaceholders=True, transient=False), 680, 40, 3)
    script("feedback", 'exec ./kraely-upload --feedback', 900, 160)
    condition("feedback-mode", "{var:kraely_native_notification}", "macOS notification", "Done", 1110, 160)
    add("notification", "output.notification", dict(lastpathcomponent=False,
        onlyshowifquerydifferent=False, removeextension=False,
        text="{var:kraely_message}", title="Upload to Remote"), 1320, 100)
    link("universal", "upload")
    link("external", "upload")
    link("upload", "success")
    link("success", "copy", "success-yes")
    link("success", "feedback")
    link("copy", "feedback")
    link("feedback", "feedback-mode")
    link("feedback-mode", "notification", "feedback-mode-yes")

    fields = []
    def field(variable, label, default, description, choices=None, placeholder=""):
        config = dict(default=default, required=False)
        if choices:
            config["pairs"] = choices  # [display label, stored value]
        else:
            config.update(placeholder=placeholder, trim=True)
        fields.append(dict(variable=variable, label=label, description=description,
            type="popupbutton" if choices else "textfield", config=config))

    field("KRAELY_HOST", "SSH host", "", "An alias from ~/.ssh/config or user@hostname. Uses your SSH config, keys, and agent.",
          placeholder="my-server")
    field("KRAELY_REMOTE_DIR", "Remote folder", "/home/ubuntu/screenshot/", "Existing absolute folder on the VPS. Files keep their names; matching names are overwritten.")
    field("KRAELY_MAX_SIZE_MB", "Max file size (MB)", "20", "Files larger than this limit are rejected before upload.")
    field("KRAELY_TIMEOUT", "Timeout (seconds)", "60", "Maximum upload duration, from 1 to 600 seconds.")
    field("KRAELY_FEEDBACK", "Status messages", "hud", "Compact progress and completion popup without taking keyboard focus.",
          [["Floating popup", "hud"], ["macOS notification", "notification"], ["Off", "off"]])
    return dict(bundleid=BUNDLE, category="Productivity", connections=connections,
        createdby="ariestwn", description="Upload a selected file to your VPS over SSH and copy its absolute remote path.",
        disabled=False, name="Upload to Remote", objects=objects,
        readme=(ROOT / "README.md").read_text(), uidata=positions,
        userconfigurationconfig=fields, variables={}, variablesdontexport=[],
        version="0.2.0", webaddress="")


def main():
    WORKFLOW.mkdir(exist_ok=True)
    (WORKFLOW / "info.plist").write_bytes(plistlib.dumps(metadata(), sort_keys=False))
    binary = WORKFLOW / "kraely-upload"
    if not binary.is_file():
        print("Generated workflow/info.plist; run ./build.sh to compile and package.")
        return
    out = ROOT / "dist/Upload to Remote.alfredworkflow"
    out.parent.mkdir(exist_ok=True)
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in [WORKFLOW / "info.plist", binary, WORKFLOW / "icon.png", ROOT / "README.md", ROOT / "LICENSE"]:
            archive.write(path, path.name)
    print(f"Built {out}")


if __name__ == "__main__":
    main()
