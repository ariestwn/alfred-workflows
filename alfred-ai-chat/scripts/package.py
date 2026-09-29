#!/usr/bin/env python3
"""Generate the workflow graph and a clean importable archive."""
from pathlib import Path
import plistlib
import uuid
import zipfile

ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = ROOT / "workflow"
BUNDLE = "com.ariestwn.ai-chat"
FILES = ["info.plist", "ai-chat", "chat-view", "chat.py", "icon.png"]


def uid(name):
    return str(uuid.uuid5(uuid.NAMESPACE_URL, BUNDLE + "/" + name)).upper()


def metadata():
    objects, connections, positions = [], {}, {}

    def add(name, kind, config, x, y, version=1):
        objects.append(dict(uid=uid(name), type="alfred.workflow." + kind, config=config, version=version))
        positions[uid(name)] = dict(xpos=x, ypos=y)

    def link(source, target, modifiers=0, title="", close=True):
        connections.setdefault(uid(source), []).append(dict(destinationuid=uid(target),
            modifiers=modifiers, modifiersubtext=title, vitoclose=close))

    def script(name, action, x, y):
        add(name, "action.script", dict(concurrently=False, escaping=0,
            script='exec ./ai-chat ' + action + ' "$1"', scriptargtype=1,
            scriptfile="", type=0), x, y, 2)

    for row, (provider, title, keyword) in enumerate([
            ("default", "AI Chat", "AI_KEYWORD"), ("codex", "Ask Codex", "AI_CODEX_KEYWORD"),
            ("claude", "Ask Claude", "AI_CLAUDE_KEYWORD"), ("opencode", "Ask OpenCode", "AI_OPENCODE_KEYWORD")]):
        y = row * 320
        add(provider + "-keyword", "input.keyword", dict(argumenttype=1,
            keyword="{var:" + keyword + "}", text=title, withspace=True, skipuniversalaction=True,
            subtext="↩ Continue chat · ⌘↩ New chat · ⌥↩ History · fn↩ Compose multiline"), 40, y)
        for row_offset, (suffix, target) in enumerate([("open", "view"), ("new", "new"), ("compose", "compose")]):
            variables = {"AI_POLL": ""}
            if provider != "default":
                variables["AI_PROVIDER"] = provider
            add(provider + "-" + suffix, "utility.argument", dict(argument="{query}",
                passthroughargument=False, variables=variables), 280, y + row_offset * 100)
            link(provider + "-" + suffix, target)
        link(provider + "-keyword", provider + "-open")
        link(provider + "-keyword", provider + "-new", 1048576, "Start new chat")
        link(provider + "-keyword", "history", 524288, "Browse chat history")
        link(provider + "-keyword", provider + "-compose", 8388608, "Compose a multiline message")
        add(provider + "-external", "trigger.external", dict(availableviaurlhandler=False,
            triggerid="ask_" + ("ai" if provider == "default" else provider)), 40, y + 260)
        link(provider + "-external", provider + "-open")
        if provider != "default":
            add(provider + "-selection", "trigger.universalaction", dict(acceptsfiles=False,
                acceptsmulti=False, acceptstext=True, acceptsurls=False, name=title), 40, y + 100)
            add(provider + "-hotkey", "trigger.hotkey", dict(action=0, argument=1,
                focusedappvariable=False, focusedappvariablename="", hotkey=0, hotmod=0,
                hotstring="", leftcursor=False, modsmode=0, relatedAppsMode=0), 40, y + 200, 2)
            link(provider + "-selection", provider + "-new")
            link(provider + "-hotkey", provider + "-new")

    add("view", "userinterface.text", dict(behaviour=2, fontmode=0, fontsizing=0,
        footertext="↩ Ask · fn↩ Compose · ⌘↩ New chat · ⌥↩ Copy last · ⌃↩ Copy all · ⇧↩ Stop",
        inputfile="chat-view", inputtype=1, loadingtext="Opening AI Chat…", outputmode=1,
        scriptinput=2, spellchecking=0, stackview=False), 620, 300)
    for row, (action, modifier, title, close) in enumerate([
            ("new", 1048576, "Start new chat", True), ("stop", 131072, "Stop answer", True),
            ("copy-last", 524288, "Copy last answer", False), ("copy-all", 262144, "Copy full chat", False)]):
        script(action, action, 870, 100 + row * 140)
        link("view", action, modifier, title, close)
        link(action, "reopen" if action in ("new", "stop") else "clipboard", close=False)

    # Editable Object Input view: with a plain ↩ connection, ⇧↩ and ⌥↩ insert new lines.
    add("compose", "userinterface.text", dict(behaviour=1, fontmode=0, fontsizing=0,
        footertext="↩ Send · ⇧↩ New line", inputfile="", inputtype=0, loadingtext="",
        outputmode=0, scriptinput=0, spellchecking=0, stackview=False), 620, 60)
    link("compose", "reopen", close=False)
    # AI_POLL is "1" while an answer streams; clear it so the composed text is sent, not treated as a poll.
    add("view-compose", "utility.argument", dict(argument="{query}", passthroughargument=False,
        variables={"AI_POLL": ""}), 870, 660)
    link("view", "view-compose", 8388608, "Compose a multiline message")
    link("view-compose", "compose")

    # Re-enter through an external trigger: Alfred's object graph must stay acyclic.
    add("reopen", "output.callexternaltrigger", dict(externaltriggerid="view_chat",
        passinputasargument=True, passvariables=True, workflowbundleid="self"), 1130, 180)
    add("view-trigger", "trigger.external", dict(availableviaurlhandler=False,
        triggerid="view_chat"), 390, 230)
    link("view-trigger", "view")

    add("clipboard", "output.clipboard", dict(autopaste=False, clipboardtext="{query}",
        ignoredynamicplaceholders=True, transient=False), 1130, 450, 3)
    add("history", "input.scriptfilter", dict(alfredfiltersresults=True,
        alfredfiltersresultsmatchmode=0, argumenttreatemptyqueryasnil=True,
        argumenttrimmode=0, argumenttype=1, escaping=0, keyword="{var:AI_HISTORY_KEYWORD}",
        queuedelaycustom=1, queuedelayimmediatelyinitially=True, queuedelaymode=0,
        queuemode=1, runningsubtext="Loading conversations…", script="exec ./ai-chat history",
        scriptargtype=1, scriptfile="", skipuniversalaction=True, subtext="Resume an archived conversation",
        title="AI Chat History", type=0, withspace=True), 620, 780, 3)
    script("restore", "restore", 870, 780)
    link("history", "restore")
    link("restore", "reopen")

    fields = []
    def field(variable, label, default="", description="", choices=None):
        config = dict(default=default, required=False)
        if choices:
            config["pairs"] = [[label, value] for value, label in choices]
        else:
            config.update(placeholder="", trim=True)
        fields.append(dict(variable=variable, label=label, description=description,
            type="popupbutton" if choices else "textfield", config=config))

    field("AI_PROVIDER", "Default provider", "codex", "Used by the ai keyword.",
          [("codex", "Codex"), ("claude", "Claude Code"), ("opencode", "OpenCode")])
    for variable, label, default in [("AI_KEYWORD", "Default keyword", "ai"),
            ("AI_CODEX_KEYWORD", "Codex keyword", "codex"), ("AI_CLAUDE_KEYWORD", "Claude keyword", "claude"),
            ("AI_OPENCODE_KEYWORD", "OpenCode keyword", "opencode"),
            ("AI_HISTORY_KEYWORD", "History keyword", "aihistory")]:
        field(variable, label, default)
    codex_models = [("gpt-5.6-sol", "GPT-5.6 Sol"), ("gpt-5.6-luna", "GPT-5.6 Luna"),
                    ("gpt-5.6-terra", "GPT-5.6 Terra"), ("gpt-6-astra", "GPT-6 Astra"),
                    ("gpt-5.5", "GPT-5.5"), ("", "CLI default")]
    claude_models = [("", "CLI default"), ("sonnet", "Sonnet"), ("opus", "Opus"), ("haiku", "Haiku")]
    opencode_models = [("deepseek/deepseek-flash", "DeepSeek V4.1 Flash"),
                       ("deepseek/deepseek-v4-flash", "DeepSeek V4 Flash"),
                       ("deepseek/deepseek-v4-pro", "DeepSeek V4 Pro"), ("", "CLI default")]
    for provider, label, default, models, effort_default in [
            ("CODEX", "Codex", "gpt-5.6-sol", codex_models, "high"),
            ("CLAUDE", "Claude", "", claude_models, "default"),
            ("OPENCODE", "OpenCode", "deepseek/deepseek-flash", opencode_models, None)]:
        field("AI_" + provider + "_MODEL", label + " model", default,
              "Choose a model for this provider. Availability depends on your CLI account.", models)
        if effort_default is not None:
            values = ["default", "low", "medium", "high", "xhigh", "max"]
            if provider == "CODEX":
                values += ["none", "minimal", "ultra"]
            field("AI_" + provider + "_EFFORT", label + " effort", effort_default,
                  "Choose a level supported by your model and CLI.",
                  [(value, "CLI default" if value == "default" else value.capitalize()) for value in values])
        field("AI_" + provider + "_MODEL_OVERRIDE", label + " custom model", "",
              "Optional exact model ID. When set, this overrides the model dropdown above.")
    field("AI_CONTEXT", "Context (prior messages)", "24", "2–100 prior messages, plus your new question.")
    field("AI_TIMEOUT", "Timeout (seconds)", "180", "Total time allowed for an answer, 1–1800 seconds.")
    fields.append(dict(variable="AI_KEEP_HISTORY", label="Keep history", description="",
        type="checkbox", config=dict(default=True, required=False, text="Archive the current conversation when starting a new one")))
    fields.append(dict(variable="AI_SYSTEM_PROMPT", label="Additional instructions",
        description="Applied to all providers.", type="textarea",
        config=dict(default="", required=False, trim=True, verticalsize=4)))
    for provider, label in [("CODEX", "Codex"), ("CLAUDE", "Claude"), ("OPENCODE", "OpenCode")]:
        field("AI_" + provider + "_PATH", label + " executable", "",
              "Optional absolute executable path. Leave blank to auto-detect.")
    return dict(bundleid=BUNDLE, category="Productivity", connections=connections,
        createdby="ariestwn", description="Chat with Codex, Claude Code, and OpenCode using your CLI login.",
        disabled=False, name="AI Chat — Codex, Claude & OpenCode", objects=objects,
        readme=(ROOT / "README.md").read_text(), uidata=positions, userconfigurationconfig=fields,
        variables={}, variablesdontexport=[], version="1.0.1", webaddress="")


def main():
    for name in ("ai-chat", "chat-view"):
        (WORKFLOW / name).chmod(0o755)
    (WORKFLOW / "info.plist").write_bytes(plistlib.dumps(metadata(), sort_keys=False))
    output = ROOT / "dist" / "AI Chat.alfredworkflow"
    output.parent.mkdir(exist_ok=True)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name in FILES:
            archive.write(WORKFLOW / name, name)
        for name in ("README.md", "LICENSE"):
            archive.write(ROOT / name, name)
    print(output)


if __name__ == "__main__":
    main()
