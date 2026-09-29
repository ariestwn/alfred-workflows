#!/usr/bin/env python3
"""Build Alfred's metadata and archive. Runtime logic lives in the Rust binary."""
from pathlib import Path
import plistlib
import uuid
import zipfile

ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = ROOT / "workflow"
IMPROVE_PROMPT = "Tone: Conversational, like talking to a friend.  - Use natural connectors when possible . - Slightly longer than punchy is fine, but still tight. No filler. - Should sound spoken, not written. all lowercase and rephrase into casual."

def uid(name):
    return str(uuid.uuid5(uuid.NAMESPACE_URL, "com.ariestwn.quickai/" + name)).upper()

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

    def condition(name, variable, choices, otherwise, x, y):
        add(name, "utility.conditional", dict(conditions=[dict(inputstring=variable,
            matchcasesensitive=True, matchmode=0, matchstring=value, outputlabel=label,
            uid=uid(name + "-" + value)) for value, label in choices],
            elselabel=otherwise, hideelse=False), x, y)

    def script(name, command, x, y):
        add(name, "action.script", dict(concurrently=False, escaping=0,
            script=command, scriptargtype=1, scriptfile="", type=0), x, y, 2)

    def copy_selection(name, x, y):
        add(name, "output.dispatchkeycombo", dict(count=1, keychar="c", keycode=8,
            keymod=1048576, overridewithargument=False), x, y)

    commands = [
        ("quickfix", "Quickfix", "Fix typos, grammar, and awkward phrasing with minimal edits"),
        ("improve-writing", "Improve Writing", "Rephrase casually in lowercase, like talking to a friend"),
        ("grammar-check", "Grammar Check", "Correct spelling, punctuation, and grammar only"),
    ]
    for row, (command, title, description) in enumerate(commands):
        y = row * 400
        add(command + "-keyword", "input.keyword", dict(argumenttype=1, keyword=command,
            subtext=description + " · Selection or typed text", text=title, withspace=True), 40, y)
        add(command + "-universal", "trigger.universalaction", dict(acceptsfiles=False,
            acceptsmulti=False, acceptstext=True, acceptsurls=False, name="QuickAI · " + title), 40, y + 100)
        add(command + "-hotkey", "trigger.hotkey", dict(action=0, argument=1,
            focusedappvariable=False, focusedappvariablename="", hotkey=0, hotmod=0, hotstring="",
            leftcursor=False, modsmode=0, relatedAppsMode=0), 40, y + 200, 2)
        add(command + "-external", "trigger.external", dict(availableviaurlhandler=True,
            triggerid=command), 40, y + 300)
        script(command + "-run", f'exec ./quickai {command} --workflow -- "$1"', 300, y + 100)
        for trigger in ["universal", "hotkey"]:
            link(command + "-" + trigger, command + "-run")
        link(command + "-run", "success")
        condition(command + "-input", "{query}", [("", "Selected text")], "Typed text", 240, y)
        link(command + "-keyword", command + "-input")
        link(command + "-external", command + "-input")
        link(command + "-input", command + "-capture", command + "-input-")
        link(command + "-input", command + "-run")
        script(command + "-capture", 'exec ./quickai --capture-start -- "$1"', 430, y)
        condition(command + "-ready", "{var:quickai_capture_ready}", [("1", "Ready")], "Error", 620, y)
        link(command + "-capture", command + "-ready")
        link(command + "-ready", command + "-copy", command + "-ready-1")
        link(command + "-ready", "feedback")
        copy_selection(command + "-copy", 810, y)
        script(command + "-captured", f'exec ./quickai {command} --workflow --capture', 1000, y)
        link(command + "-copy", command + "-captured")
        link(command + "-captured", "success")

    condition("success", "{var:quickai_ok}", [("1", "Success")], "Error", 1220, 500)
    condition("verify", "{var:quickai_verify}", [("1", "Confirm selection")], "Copy only", 1410, 350)
    condition("output", "{var:quickai_output}", [("copy", "Copy"), ("both", "Paste and copy")], "Paste", 1980, 550)
    link("success", "verify", "success-1")
    link("success", "feedback")
    link("verify", "verify-start", "verify-1")
    link("verify", "output")
    script("verify-start", 'exec ./quickai --capture-start -- "$1"', 1410, 150)
    condition("verify-ready", "{var:quickai_capture_ready}", [("1", "Ready")], "Copy result", 1600, 150)
    link("verify-start", "verify-ready")
    link("verify-ready", "verify-copy", "verify-ready-1")
    link("verify-ready", "verify-finish")
    copy_selection("verify-copy", 1790, 150)
    script("verify-finish", 'exec ./quickai --verify-paste -- "$1"', 1980, 300)
    link("verify-copy", "verify-finish")
    link("verify-finish", "output")
    for name, autopaste, transient, y in [("copy", False, False, 40), ("both", True, False, 240), ("paste", True, True, 440)]:
        add(name, "output.clipboard", dict(autopaste=autopaste, clipboardtext="{query}",
            ignoredynamicplaceholders=True, transient=transient), 2200, y, 3)
        link("output", name, "output-" + name if name != "paste" else None)
        link(name, "feedback")
    script("feedback", 'exec ./quickai --feedback', 2440, 600)
    condition("feedback-mode", "{var:quickai_native_notification}", [("1", "macOS notification")], "Done", 2630, 600)
    link("feedback", "feedback-mode")
    link("feedback-mode", "notification", "feedback-mode-1")
    add("notification", "output.notification", dict(lastpathcomponent=False,
        onlyshowifquerydifferent=False, removeextension=False,
        text="{var:quickai_message}", title="QuickAI"), 2820, 600)

    fields = []
    def field(variable, label, default="", description="", choices=None):
        config = dict(default=default, required=False)
        if choices:
            # Alfred serializes each pair as [display label, stored value].
            config["pairs"] = [[label, value] for value, label in choices]
        else:
            config.update(placeholder="", trim=True)
        fields.append(dict(variable=variable, label=label, description=description,
            type="popupbutton" if choices else "textfield", config=config))

    field("QUICKAI_PROVIDER", "Provider", "codex", "Choose which signed-in CLI rewrites your text.",
        [["codex", "Codex"], ["claude", "Claude Code"], ["opencode", "OpenCode"]])
    for provider, label, default, effort_default in [
            ("CODEX", "Codex", "gpt-5.6-luna", "medium"),
            ("CLAUDE", "Claude", "", "low"),
            ("OPENCODE", "OpenCode", "deepseek/deepseek-flash", None)]:
        description = "Leave blank for the CLI default, or enter a model ID"
        if provider == "CLAUDE":
            description += " / alias such as sonnet."
        elif provider == "OPENCODE":
            description += ", for example deepseek/deepseek-flash."
        else:
            description += "."
        field(f"QUICKAI_{provider}_MODEL", label + " model", default, description)
        if effort_default is None:
            continue
        values = ["default", "low", "medium", "high", "xhigh", "max"]
        if provider == "CODEX":
            values += ["none", "minimal", "ultra"]
        field(f"QUICKAI_{provider}_EFFORT", label + " effort", effort_default,
            "Available effort levels depend on the selected model and CLI version.",
            [[v, "CLI default" if v == "default" else v.capitalize()] for v in values])
    field("QUICKAI_OUTPUT", "Output", "paste", "If the original selection cannot be confirmed, the result is copied.",
        [["paste", "Paste (replace selection)"], ["copy", "Copy to clipboard"], ["both", "Paste and copy"]])
    field("QUICKAI_TIMEOUT", "Timeout (seconds)", "120", "Maximum request duration: 1–600 seconds.")
    field("QUICKAI_FEEDBACK", "Status messages", "hud", "The popup shows progress without taking keyboard focus. Falls back to a macOS notification if unavailable.",
        [["hud", "Floating popup"], ["notification", "macOS notification"], ["off", "Off"]])
    field("QUICKAI_GLOBAL_PROMPT", "Additional writing rules", "", "Optional rules applied to all three commands.")
    for provider, label in [("CODEX", "Codex"), ("CLAUDE", "Claude"), ("OPENCODE", "OpenCode")]:
        field(f"QUICKAI_{provider}_PATH", label + " executable", "",
            "Optional absolute path to the CLI executable. Leave blank to auto-detect.")
    for variable, title in [("QUICKFIX", "Quickfix"), ("IMPROVE", "Improve Writing"), ("GRAMMAR", "Grammar Check")]:
        field(f"QUICKAI_{variable}_PROMPT", title + " prompt override",
            IMPROVE_PROMPT if variable == "IMPROVE" else "", "Leave blank to use the built-in editing prompt.")

    return dict(bundleid="com.ariestwn.quickai", category="Productivity", connections=connections,
        createdby="ariestwn", description="Quickfix, Improve Writing, and Grammar Check using Codex, Claude Code, or OpenCode.",
        disabled=False, name="QuickAI", objects=objects, readme=(ROOT / "README.md").read_text(),
        uidata=positions, userconfigurationconfig=fields,
        variables={}, variablesdontexport=[], version="0.2.0", webaddress="")

def main():
    WORKFLOW.mkdir(exist_ok=True)
    info = metadata()
    (WORKFLOW / "info.plist").write_bytes(plistlib.dumps(info, sort_keys=False))
    binary = WORKFLOW / "quickai"
    if not binary.is_file():
        print("Generated workflow/info.plist; run ./build.sh to compile and package.")
        return
    out = ROOT / "dist" / "QuickAI.alfredworkflow"
    out.parent.mkdir(exist_ok=True)
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in [WORKFLOW / "info.plist", binary, WORKFLOW / "icon.png", ROOT / "README.md", ROOT / "LICENSE"]:
            archive.write(path, path.name)
    print(f"Built {out}")

if __name__ == "__main__":
    main()
