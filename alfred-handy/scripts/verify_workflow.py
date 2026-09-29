#!/usr/bin/env python3
"""Validate the shipped graph and read-only CLI against an isolated fixture."""
from pathlib import Path
import json
import os
import plistlib
import sqlite3
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent

def main():
    with zipfile.ZipFile(ROOT/"dist/Handy Rust.alfredworkflow") as z:
        assert z.testzip() is None
        assert set(z.namelist()) == {"handy","info.plist","icon.png","README.md","LICENSE","THIRD_PARTY_NOTICES.md"}
        assert z.getinfo("handy").external_attr >> 16 & 0o111
        plist = plistlib.loads(z.read("info.plist"))
        assert plist["bundleid"] == "com.ariestwn.handy-rust"
        assert z.read("handy") == (ROOT/"workflow/handy").read_bytes()
    objects = {obj["uid"]:obj for obj in plist["objects"]}
    assert len(objects) == len(plist["objects"])
    for source, edges in plist["connections"].items():
        assert source in objects
        ports = {p["uid"] for p in objects[source]["config"].get("conditions",[])}
        for edge in edges:
            assert edge["destinationuid"] in objects
            if "sourceoutputuid" in edge: assert edge["sourceoutputuid"] in ports
    route = next(obj for obj in objects.values() if obj["type"].endswith("utility.conditional"))
    routes = {p["matchstring"]:p["uid"] for p in route["config"]["conditions"]}
    assert set(routes) == {"browse","copy","paste","view","reveal","open"}
    else_edges = [e for e in plist["connections"][route["uid"]] if "sourceoutputuid" not in e]
    assert len(else_edges) == 1
    assert objects[else_edges[0]["destinationuid"]]["type"].endswith("output.notification")
    for obj in objects.values():
        if obj["type"].endswith("output.clipboard"):
            assert obj["config"]["ignoredynamicplaceholders"]
        if "script" in obj["config"]:
            assert obj["config"]["scriptargtype"] == 1
            assert '"$1"' in obj["config"]["script"]
            assert "{query}" not in obj["config"]["script"]
    with tempfile.TemporaryDirectory(prefix="handy-verify-") as tmp:
        home = Path(tmp)
        data = home/"data"
        data.mkdir()
        (data/"recordings").mkdir()
        (data/"settings_store.json").write_text(json.dumps({"settings":{"selected_model":"small","selected_language":"auto","custom_words":["Codex","日本語"]}}))
        conn = sqlite3.connect(data/"history.db")
        conn.execute("CREATE TABLE transcription_history (id INTEGER PRIMARY KEY, file_name TEXT, timestamp INTEGER, saved INTEGER, title TEXT, transcription_text TEXT, post_processed_text TEXT)")
        conn.executemany("INSERT INTO transcription_history VALUES (?,?,?,0,?,?,NULL)", [(i,f"{i}.wav",i,"Test transcript",f"Original {i} 日本語") for i in range(1,106)])
        conn.commit()
        conn.close()
        env = dict(os.environ,HOME=tmp,HANDY_DATA_DIR=str(data),HANDY_HF_CACHE=str(home/"hub"),HANDY_PAGE_SIZE="40",HANDY_PAGE="0",HANDY_PAGE_QUERY="")
        def call(*args, extra=None):
            return json.loads(subprocess.check_output([str(ROOT/"workflow/handy"),*args],env=dict(env,**(extra or {}))))
        for query in ["","history ","saved ","models ","languages ","dictionary ","add test","entry 1","delete 1"]:
            result = call("filter","--",query)
            assert isinstance(result["items"],list) and result["items"]
            assert all("title" in item and "valid" in item for item in result["items"])
            for item in result["items"]:
                if item["valid"]: assert "op" in json.loads(item["arg"])
        first = call("filter","--","history ")
        assert len(first["items"]) == 41
        assert json.loads(first["items"][0]["arg"])["id"] == 105
        navigation = call("action","--",first["items"][-1]["arg"])["alfredworkflow"]
        second = call("filter","--",navigation["arg"],extra=navigation["variables"])
        assert json.loads(second["items"][1]["arg"])["id"] == 65
        trimmed = call("filter","--",navigation["arg"].rstrip(),extra=navigation["variables"])
        assert json.loads(trimmed["items"][1]["arg"])["id"] == 65
        searched = call("filter","--","history Original 105",extra=navigation["variables"])
        assert len(searched["items"]) == 1
        for op,expected in [("copy","copy"),("paste","paste"),("view","view"),("raw","copy")]:
            assert call("action","--",json.dumps({"op":op,"id":1}))["alfredworkflow"]["variables"]["HANDY_ROUTE"] == expected
        failed = call("action","--",'{"op":"copy","id":999}')
        assert failed["alfredworkflow"]["variables"]["HANDY_ROUTE"] == "error"
        assert failed["alfredworkflow"]["arg"] == ""
        word = call("filter","add","--","--")
        assert json.loads(word["items"][0]["arg"])["value"] == "--"
    print(f"Verified archive, {len(objects)} Alfred objects, routing, and isolated CLI navigation.")

if __name__ == "__main__": main()
