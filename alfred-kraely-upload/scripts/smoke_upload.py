#!/usr/bin/env python3
"""Run real scp/SFTP locally, with synthetic files and no SSH/network/clipboard."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "target/debug/alfred-kraely-upload"
with tempfile.TemporaryDirectory(prefix="kraely-sftp-") as tmp:
    root = Path(tmp)
    source = root / "source"
    remote = root / "remote team's files"
    source.mkdir()
    remote.mkdir()
    wrapper = root / "scp-local"
    wrapper.write_text('#!/bin/sh\nexec /usr/bin/scp -D /usr/libexec/sftp-server "$@"\n')
    wrapper.chmod(0o700)
    env = dict(os.environ, KRAELY_TEST_SCP=str(wrapper), KRAELY_HOST="local-test",
               KRAELY_REMOTE_DIR=str(remote) + "/", KRAELY_FEEDBACK="off", KRAELY_TIMEOUT="5")

    def invoke(paths, **overrides):
        result = subprocess.run([BINARY, "--workflow", "--", *paths],
            env=dict(env, **overrides), capture_output=True, text=True, timeout=10, check=True)
        return json.loads(result.stdout)["alfredworkflow"]

    names = ["Screenshot 2026-09-13 at 20.30.00.png", "-report: it's {query} $(id) `id` 🦀.txt"]
    for name in names:
        local = source / name
        contents = b"Synthetic uploader fixture\n\x00\xff"
        local.write_bytes(contents)
        output = invoke([local])
        assert output["variables"]["kraely_ok"] == "1", output
        assert output["arg"] == str(remote / name), output
        assert (remote / name).read_bytes() == contents
        local.write_bytes(b"replacement")
        assert invoke([local])["variables"]["kraely_ok"] == "1"
        assert (remote / name).read_bytes() == b"replacement"
    print("Real local SFTP: spaces, Unicode, literal shell characters, exact copied path, overwrite passed.")
    link = source / "friendly name.txt"
    link.symlink_to(source / names[0])
    output = invoke([link])
    assert output["variables"]["kraely_ok"] == "1", output
    assert output["arg"] == str(remote / link.name)
    assert (remote / link.name).read_bytes() == link.read_bytes()
    collision = source / "existing-directory"
    collision.write_bytes(b"must not be nested")
    (remote / collision.name).mkdir()
    output = invoke([collision])
    assert output["variables"]["kraely_ok"] == "0", output
    assert output["arg"] == ""
    assert list((remote / collision.name).iterdir()) == []
    print("Preserved a selected symlink's filename and rejected a same-named remote directory.")
    local = source / names[0]
    for paths, overrides in [
        ([], {}), ([local, local], {}), ([source], {}),
        ([source / "missing"], {}),
        ([local], {"KRAELY_MAX_SIZE_MB": "0.000001"}),
        ([local], {"KRAELY_REMOTE_DIR": str(root / "missing-remote-dir")}),
        ([local], {"KRAELY_HOST": "-oProxyCommand=bad"}),
    ]:
        output = invoke(paths, **overrides)
        assert output["variables"]["kraely_ok"] == "0", output
        assert output["arg"] == "", output
    print("Rejected invalid inputs/settings, oversize file, and missing remote directory without copying a path.")
