#!/usr/bin/env python3
"""One real request using isolated test data; never touches Alfred's chat history."""
import os
from pathlib import Path
import sys
import tempfile
import time
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "workflow"))
from chat import Chat, read_json

provider = sys.argv[1]
with tempfile.TemporaryDirectory(prefix="alfred-ai-chat-smoke-") as directory:
    env = dict(os.environ, AI_PROVIDER=provider, AI_TIMEOUT="60", AI_CONTEXT="2",
               alfred_workflow_data=directory + "/data", alfred_workflow_cache=directory + "/cache")
    chat = Chat(env)
    try:
        result = chat.view("Reply with exactly ALFRED_OK and nothing else.")
        started = time.monotonic()
        while result.get("rerun") and time.monotonic() - started < 65:
            time.sleep(0.2)
            result = chat.view()
        saved = read_json(chat.path, {})
        answer = chat.action("copy-last")
        if answer.strip() != "ALFRED_OK":
            raise SystemExit(provider + " smoke test failed: " + (saved.get("error") or result["response"]))
        print(provider + ": " + answer.strip())
    finally:
        chat.action("stop")
