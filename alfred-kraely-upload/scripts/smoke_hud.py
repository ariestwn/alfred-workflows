#!/usr/bin/env python3
"""Exercise the real native HUD, without typing, clipboard writes, or network calls.

Requires a logged-in macOS desktop. Debug builds also render their own view to
temporary PNGs; this does not capture other apps or require Screen Recording.
"""
import ctypes
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
BINARY = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "target/debug/alfred-kraely-upload"
TEMP = Path(os.environ.get("TMPDIR", tempfile.gettempdir()))
PREVIEWS = Path(tempfile.mkdtemp(prefix="kraely-preview-"))
objc = ctypes.CDLL("/usr/lib/libobjc.A.dylib")
ctypes.CDLL("/System/Library/Frameworks/AppKit.framework/AppKit")
objc.objc_getClass.argtypes = [ctypes.c_char_p]
objc.objc_getClass.restype = ctypes.c_void_p
objc.sel_registerName.argtypes = [ctypes.c_char_p]
objc.sel_registerName.restype = ctypes.c_void_p
send = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p)(("objc_msgSend", objc))
send_int = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p)(("objc_msgSend", objc))


def frontmost():
    workspace = send(objc.objc_getClass(b"NSWorkspace"), objc.sel_registerName(b"sharedWorkspace"))
    app = send(workspace, objc.sel_registerName(b"frontmostApplication"))
    return send_int(app, objc.sel_registerName(b"processIdentifier"))


observed = set()
helpers = set()


def wait_for(predicate, timeout=12):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        observed.add(frontmost())
        assert not (observed & helpers), "HUD took application focus"
        if predicate():
            return
        time.sleep(0.04)
    raise AssertionError("HUD test timed out")


def save_state(directory, state):
    staged = directory / "next.json"
    staged.write_text(json.dumps(state))
    staged.replace(directory / "state.json")


def direct_case(phase, detail, interrupt=False):
    directory = Path(tempfile.mkdtemp(prefix="kraely-hud-", dir=TEMP))
    state = dict(phase="working", title="Uploading to remote…", detail="Upload to Remote",
                 owner=os.getpid(), expires=int(time.time() * 1000) + 15000)
    save_state(directory, state)
    env = dict(os.environ, KRAELY_HUD_SNAPSHOT_DIR=str(PREVIEWS))
    process = subprocess.Popen([BINARY, "--hud", directory], env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    helpers.add(process.pid)
    try:
        wait_for(lambda: (directory / "ready").exists() or process.poll() is not None, 4)
        assert (directory / "ready").exists(), process.communicate(timeout=2)[1]
        time.sleep(0.2)
        if interrupt:
            state.update(owner=0, expires=0)
        else:
            state.update(phase=phase, title="Upload to Remote · Done" if phase == "success" else "Upload to Remote couldn’t finish",
                         detail=detail, owner=0)
        save_state(directory, state)
        wait_for(lambda: process.poll() is not None)
        _, error = process.communicate(timeout=2)
        assert process.returncode == 0, error
        assert not directory.exists(), "HUD left its state files behind"
        print(f"Verified native {'interruption' if interrupt else phase}: no activation, auto-dismiss, cleanup.", flush=True)
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        shutil.rmtree(directory, ignore_errors=True)


def workflow_case():
    with tempfile.TemporaryDirectory(prefix="kraely-mock-") as temp:
        mock = Path(temp) / "scp"
        mock.write_text("#!/bin/sh\nsleep 0.4\nexit 0\n")
        mock.chmod(0o755)
        source = Path(temp) / "sample image.png"
        source.write_text("Synthetic upload test. Never sent to a server.")
        env = dict(os.environ, KRAELY_TEST_SCP=str(mock), KRAELY_HOST="mock-host",
                   KRAELY_REMOTE_DIR="/srv/test/", KRAELY_FEEDBACK="hud", KRAELY_TIMEOUT="5")
        process = subprocess.Popen([BINARY, "--workflow", "--", source],
                                   env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        wait_for(lambda: process.poll() is not None, 10)
        output, error = process.communicate(timeout=2)
        assert process.returncode == 0, error
        result = json.loads(output)["alfredworkflow"]
        assert result["arg"] == "/srv/test/sample image.png"
        variables = result["variables"]
        assert variables["kraely_ok"] == "1"
        directory = Path(variables["kraely_hud"])
        assert str(directory) != ".", error
        helpers.add(int((directory / "ready").read_text()))
        state = json.loads((directory / "state.json").read_text())
        assert state["owner"] == 0 and state["phase"] == "working"
        assert "Synthetic" not in json.dumps(state), "File contents leaked to HUD state"
        env.update(variables)
        finished = subprocess.run([BINARY, "--feedback"], env=env, text=True,
                                  capture_output=True, timeout=5, check=True)
        assert json.loads(finished.stdout)["alfredworkflow"]["variables"]["kraely_native_notification"] == "0"
        assert json.loads((directory / "state.json").read_text())["phase"] == "success"
        wait_for(lambda: not directory.exists())
        print("Verified workflow → HUD handoff → completion with mock scp; no clipboard writes.", flush=True)
        for mode, expected in [("notification", "1"), ("off", "0")]:
            env["KRAELY_FEEDBACK"] = mode
            response = subprocess.run([BINARY, "--feedback"], env=env, text=True,
                                      capture_output=True, timeout=5, check=True)
            assert json.loads(response.stdout)["alfredworkflow"]["variables"]["kraely_native_notification"] == expected
        print("Verified macOS notification and Off settings.", flush=True)


assert frontmost() > 0, "This test needs access to the logged-in macOS desktop"
direct_case("success", "Remote path copied. Paste with ⌘V.")
direct_case("error", "SSH authentication failed. Make sure key-based login works without prompting.")
direct_case("error", "", interrupt=True)
workflow_case()
assert not (observed & helpers)
print(f"All HUD smoke checks passed. View-only renders: {PREVIEWS}", flush=True)
