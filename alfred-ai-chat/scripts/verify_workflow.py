#!/usr/bin/env python3
from pathlib import Path
import plistlib
import stat
import zipfile
from package import BUNDLE, FILES, ROOT, uid

with zipfile.ZipFile(ROOT / "dist/AI Chat.alfredworkflow") as archive:
    assert set(archive.namelist()) == set(FILES + ["README.md", "LICENSE"])
    assert archive.testzip() is None
    data = plistlib.loads(archive.read("info.plist"))
    assert data["bundleid"] == BUNDLE
    assert data["bundleid"] != "com.alfredapp.vitor.openai"
    assert not data["variables"]
    for name in ("ai-chat", "chat-view"):
        assert archive.getinfo(name).external_attr >> 16 & stat.S_IXUSR
    objects = {item["uid"]: item for item in data["objects"]}
    assert len(objects) == len(data["objects"])
    for source, links in data["connections"].items():
        assert source in objects
        for link in links:
            assert link["destinationuid"] in objects
    def visit(node, trail):
        assert node not in trail, "Alfred object graph contains a cycle"
        for edge in data["connections"].get(node, []):
            visit(edge["destinationuid"], trail | {node})
    for node in objects:
        visit(node, set())
    assert {edge["modifiers"] for edge in data["connections"][uid("view")]} == {1048576, 131072, 524288, 262144, 8388608}
    compose = [edge for edge in data["connections"][uid("view")] if edge["modifiers"] == 8388608]
    assert [edge["destinationuid"] for edge in compose] == [uid("view-compose")]
    assert objects[uid("view-compose")]["config"]["variables"] == {"AI_POLL": ""}
    assert [edge["destinationuid"] for edge in data["connections"][uid("view-compose")]] == [uid("compose")]
    for provider in ("codex", "claude", "opencode"):
        assert objects[uid(provider + "-open")]["config"]["variables"]["AI_PROVIDER"] == provider
    assert "AI_PROVIDER" not in objects[uid("default-open")]["config"]["variables"]
    for provider, trigger in [("default", "ask_ai"), ("codex", "ask_codex"),
                              ("claude", "ask_claude"), ("opencode", "ask_opencode")]:
        assert objects[uid(provider + "-external")]["config"]["triggerid"] == trigger
        assert [edge["destinationuid"] for edge in data["connections"][uid(provider + "-external")]] == [uid(provider + "-open")]
        compose = [edge for edge in data["connections"][uid(provider + "-keyword")] if edge["modifiers"] == 8388608]
        assert [edge["destinationuid"] for edge in compose] == [uid(provider + "-compose")]
        assert [edge["destinationuid"] for edge in data["connections"][uid(provider + "-compose")]] == [uid("compose")]
    editor = objects[uid("compose")]["config"]
    assert (editor["inputtype"], editor["behaviour"]) == (0, 1)
    assert [(edge["modifiers"], edge["destinationuid"]) for edge in data["connections"][uid("compose")]] == [(0, uid("reopen"))]
    variables = {field["variable"] for field in data["userconfigurationconfig"]}
    settings = {field["variable"]: field for field in data["userconfigurationconfig"]}
    assert settings["AI_CODEX_MODEL"]["config"]["default"] == "gpt-5.6-sol"
    assert settings["AI_CODEX_EFFORT"]["config"]["default"] == "high"
    assert settings["AI_OPENCODE_MODEL"]["config"]["default"] == "deepseek/deepseek-flash"
    for provider in ("CODEX", "CLAUDE", "OPENCODE"):
        assert settings["AI_" + provider + "_MODEL"]["type"] == "popupbutton"
    for provider in ("CODEX", "CLAUDE"):
        assert settings["AI_" + provider + "_EFFORT"]["type"] == "popupbutton"
        assert settings["AI_" + provider + "_PATH"]["type"] == "textfield"
    assert settings["AI_OPENCODE_PATH"]["type"] == "textfield"
    assert "AI_OPENCODE_EFFORT" not in settings
    assert len(variables) == len(data["userconfigurationconfig"])
    assert not any("api_key" in name.lower() for name in variables)
    assert objects[uid("view")]["config"]["inputfile"] in archive.namelist()
print("Workflow graph, settings, executable modes, and archive contents verified.")
