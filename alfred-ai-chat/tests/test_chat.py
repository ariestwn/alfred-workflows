import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "workflow"))
from chat import Chat, Stream, command, configuration, fresh, opencode_settings, read_json

FAKE = r'''#!/usr/bin/python3
import json, os, pathlib, subprocess, sys, time
args = sys.argv[1:]
prompt = sys.stdin.read()
if args and args[0] == "session":
    with open(os.environ["SESSION_LOG"], "a", encoding="utf-8") as log:
        log.write(json.dumps(args) + "\n")
    sys.exit(0)
pathlib.Path(os.environ["CAPTURE"]).write_text(json.dumps({"args": args, "prompt": prompt}))
mode = os.environ.get("FAKE_MODE", "success")
def event(value):
    print(json.dumps(value), flush=True)
if mode == "hang":
    child = subprocess.Popen(["/bin/sleep", "60"])
    pathlib.Path(os.environ["CHILD_PID"]).write_text(str(child.pid))
    time.sleep(60)
if mode == "error":
    if args[0] == "exec":
        event({"type": "turn.failed", "error": {"message": "Login required"}})
    elif args[0] == "run":
        event({"type": "error", "sessionID": "ses_fake", "error": {"message": "Login required"}})
    else:
        event({"type": "result", "subtype": "error_during_execution", "is_error": True, "errors": ["Login required"]})
    sys.exit(1)
if mode == "no_final":
    print("diagnostic output only")
    sys.exit(0)
time.sleep(float(os.environ.get("FAKE_DELAY", "0")))
if args[0] == "exec":
    event({"type": "item.completed", "item": {"type": "reasoning", "text": "PRIVATE REASONING"}})
    event({"type": "item.completed", "item": {"type": "agent_message", "text": "Answer ✅"}})
    if mode != "no_file":
        pathlib.Path(args[args.index("--output-last-message") + 1]).write_text("Answer ✅")
    event({"type": "turn.completed"})
elif args[0] == "run":
    event({"type": "step_start", "sessionID": "ses_fake", "part": {"type": "step-start"}})
    event({"type": "text", "sessionID": "ses_fake", "part": {"type": "reasoning", "text": "PRIVATE REASONING"}})
    event({"type": "text", "sessionID": "ses_fake", "part": {"type": "text", "text": "Answer ✅"}})
else:
    event({"type": "stream_event", "event": {"type": "content_block_delta", "delta": {"type": "thinking_delta", "thinking": "PRIVATE REASONING"}}})
    for word in ["Answer ", "✅"]:
        event({"type": "stream_event", "event": {"type": "content_block_delta", "delta": {"type": "text_delta", "text": word}}})
    event({"type": "assistant", "message": {"content": [{"type": "text", "text": "Answer ✅"}]}})
    event({"type": "result", "subtype": "success", "is_error": False, "result": "Answer ✅"})
'''


class ChatTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ai-chat-test-")
        self.root = Path(self.temp.name)
        executable = self.root / "mock cli with spaces"
        executable.write_text(FAKE)
        executable.chmod(0o755)
        self.env = dict(os.environ, AI_PROVIDER="codex", AI_CODEX_PATH=str(executable),
            AI_CLAUDE_PATH=str(executable), AI_OPENCODE_PATH=str(executable),
            AI_TIMEOUT="10", AI_CONTEXT="4",
            AI_KEEP_HISTORY="1", AI_CODEX_MODEL="", AI_CLAUDE_MODEL="", AI_OPENCODE_MODEL="",
            AI_CODEX_EFFORT="default", AI_CLAUDE_EFFORT="default", AI_OPENCODE_EFFORT="", AI_POLL="",
            alfred_workflow_data=str(self.root / "data"), alfred_workflow_cache=str(self.root / "cache"),
            CAPTURE=str(self.root / "capture.json"), CHILD_PID=str(self.root / "child.pid"),
            SESSION_LOG=str(self.root / "sessions.log"))
        self.chat = Chat(self.env)

    def tearDown(self):
        for provider in ("codex", "claude", "opencode"):
            Chat(dict(self.env, AI_PROVIDER=provider)).action("stop")
        # Let detached workers acknowledge the marker before deleting their private files.
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            states = [read_json(p, {}) for p in (self.root / "cache").glob("job-*/state.json")]
            if all(s.get("status") != "running" for s in states):
                break
            time.sleep(0.03)
        self.temp.cleanup()

    def wait(self, chat=None):
        chat = chat or self.chat
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            result = chat.view()
            if not result.get("rerun"):
                return result
            time.sleep(0.03)
        self.fail("worker did not finish")

    def test_both_providers_and_literal_unicode_prompt(self):
        for provider in ("codex", "claude", "opencode"):
            chat = Chat(dict(self.env, AI_PROVIDER=provider))
            query = "--help 日本語 'quotes' $(touch NOPE) `whoami`\n{clipboard}"
            self.assertIn("rerun", chat.view(query))
            result = self.wait(chat)
            self.assertIn("Answer ✅", result["response"])
            self.assertNotIn("PRIVATE REASONING", result["response"])
            captured = read_json(self.root / "capture.json")
            self.assertEqual(json.loads(captured["prompt"].split("\n", 1)[1])[-1]["content"], query)
            self.assertNotIn(query, captured["args"])
            self.assertEqual(chat.action("copy-last"), "Answer ✅")

    def test_reopen_and_poll_do_not_duplicate_question(self):
        self.chat.env["FAKE_DELAY"] = "0.4"
        self.chat.view("First")
        poll = Chat(dict(self.chat.env, AI_POLL="1"))
        poll.view("First")
        self.chat.view("Do not submit while busy")
        self.wait()
        saved = read_json(self.chat.path)
        self.assertEqual([m["content"] for m in saved["messages"] if m["role"] == "user"], ["First"])
        self.assertNotIn("job", saved)

    def test_followup_context_and_history_restore(self):
        self.chat.view("First")
        self.wait()
        self.chat.view("Follow up")
        self.wait()
        context = json.loads(read_json(self.root / "capture.json")["prompt"].split("\n", 1)[1])
        self.assertEqual([m["role"] for m in context], ["user", "assistant", "user"])
        ident = read_json(self.chat.path)["id"]
        result = self.chat.action("new", "New question")
        self.assertEqual(result["alfredworkflow"]["arg"], "New question")
        self.assertEqual(self.chat.history()["items"][0]["arg"], ident)
        self.chat.action("restore", ident)
        self.assertEqual(self.chat.action("copy-last"), "Answer ✅")
        self.assertIn("Follow up", self.chat.action("copy-all"))

    def test_provider_isolation_and_history_disabled(self):
        self.chat.view("Codex only")
        self.wait()
        claude = Chat(dict(self.env, AI_PROVIDER="claude"))
        self.assertNotIn("Codex only", claude.view()["response"])
        self.chat.env["AI_KEEP_HISTORY"] = "0"
        self.chat.action("new")
        self.assertFalse(self.chat.history()["items"][0]["valid"])

    def test_cli_errors_never_become_successful_answers_or_context(self):
        for provider in ("codex", "claude", "opencode"):
            chat = Chat(dict(self.env, AI_PROVIDER=provider, FAKE_MODE="error"))
            chat.view("Will fail")
            result = self.wait(chat)
            self.assertIn("Login required", result["response"])
            self.assertEqual(chat.action("copy-last"), "")
            chat.env["FAKE_MODE"] = "success"
            chat.view("Retry")
            self.wait(chat)
            context = json.loads(read_json(self.root / "capture.json")["prompt"].split("\n", 1)[1])
            self.assertEqual(context, [{"role": "user", "content": "Retry"}])

    def test_exit_zero_without_final_answer_is_an_error(self):
        for mode in ("no_final", "no_file"):
            self.chat.env["FAKE_MODE"] = mode
            self.chat.view("Question")
            self.wait()
            self.assertTrue(read_json(self.chat.path)["error"])
            self.assertEqual(self.chat.action("copy-last"), "")
            self.chat.action("new")

    def wait_child(self):
        deadline = time.monotonic() + 5
        while not (self.root / "child.pid").exists() and time.monotonic() < deadline:
            time.sleep(0.03)
        return int((self.root / "child.pid").read_text())

    def assert_child_stopped(self, pid):
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            result = subprocess.run(["/bin/ps", "-o", "stat=", "-p", str(pid)], capture_output=True, text=True)
            if not result.stdout.strip() or result.stdout.strip().startswith("Z"):
                return
            time.sleep(0.03)
        self.fail("CLI descendant survived cancellation")

    def test_timeout_stops_descendants(self):
        self.chat.env.update(FAKE_MODE="hang", AI_TIMEOUT="1")
        self.chat.view("Question")
        pid = self.wait_child()
        result = self.wait()
        self.assertIn("timed out", result["response"])
        self.assert_child_stopped(pid)

    def test_cancel_and_new_chat_cannot_be_overwritten_by_old_worker(self):
        self.chat.env["FAKE_MODE"] = "hang"
        self.chat.view("Old question")
        pid = self.wait_child()
        self.chat.action("new")
        self.chat.env["FAKE_MODE"] = "success"
        self.chat.view("New question")
        result = self.wait()
        self.assertNotIn("Old question", result["response"])
        self.assertEqual(len(read_json(self.chat.path)["messages"]), 2)
        self.assert_child_stopped(pid)

    def test_missing_cli_and_invalid_settings_do_not_add_history(self):
        self.chat.env["AI_CODEX_PATH"] = "/does/not/exist"
        self.assertIn("CLI not found", self.chat.view("Question")["response"])
        self.assertFalse(self.chat.path.exists())
        self.chat.env["AI_CODEX_PATH"] = self.env["AI_CODEX_PATH"]
        self.chat.env["AI_TIMEOUT"] = "NaN"
        self.assertIn("AI_TIMEOUT", self.chat.view("Question")["response"])
        self.assertFalse(self.chat.path.exists())

    def test_model_effort_and_no_shell_interpolation(self):
        env = dict(self.env, AI_CODEX_MODEL='model with spaces; echo nope', AI_CODEX_EFFORT="high")
        cfg = configuration("codex", env)
        args = command(cfg, self.root)
        self.assertEqual(args[args.index("--model") + 1], env["AI_CODEX_MODEL"])
        self.assertIn('model_reasoning_effort="high"', args)
        self.assertEqual(args[-1], "-")
        override = configuration("codex", dict(env, AI_CODEX_MODEL_OVERRIDE="custom-exact-model"))
        self.assertEqual(override["model"], "custom-exact-model")
        defaults = dict(self.env)
        defaults.pop("AI_CODEX_MODEL")
        defaults.pop("AI_CODEX_EFFORT")
        self.assertEqual(configuration("codex", defaults)["model"], "gpt-5.6-sol")
        self.assertEqual(configuration("codex", defaults)["effort"], "high")

    def test_opencode_defaults_and_command(self):
        defaults = {key: value for key, value in self.env.items() if key != "AI_OPENCODE_MODEL"}
        cfg = configuration("opencode", defaults)
        self.assertEqual(cfg["model"], "deepseek/deepseek-flash")
        self.assertEqual(cfg["effort"], "")
        args = command(cfg, self.root)
        self.assertEqual(args[:5], [cfg["executable"], "run", "--format", "json", "--agent"])
        self.assertEqual(args[args.index("--agent") + 1], "alfred")
        self.assertEqual(args[args.index("--model") + 1], "deepseek/deepseek-flash")
        override = configuration("opencode", dict(self.env, AI_OPENCODE_MODEL_OVERRIDE="deepseek/deepseek-v4-pro"))
        self.assertEqual(override["model"], "deepseek/deepseek-v4-pro")
        settings = opencode_settings("SYSTEM TEXT")
        deny = [{"action": "*", "resource": "*", "effect": "deny"}]
        self.assertEqual(settings["permissions"], deny)
        self.assertEqual(settings["agents"]["alfred"]["system"], "SYSTEM TEXT")
        self.assertEqual(settings["agents"]["alfred"]["permissions"], deny)

    def test_opencode_stream_keeps_final_step_text_and_session(self):
        stream = Stream("opencode")
        stream.feed(json.dumps({"type": "step_start", "sessionID": "ses_1", "part": {"type": "step-start"}}))
        stream.feed(json.dumps({"type": "text", "sessionID": "ses_1", "part": {"type": "reasoning", "text": "PRIVATE"}}))
        stream.feed(json.dumps({"type": "text", "sessionID": "ses_1", "part": {"type": "text", "text": "Draft"}}))
        stream.feed(json.dumps({"type": "step_start", "sessionID": "ses_1", "part": {"type": "step-start"}}))
        stream.feed(json.dumps({"type": "text", "sessionID": "ses_1", "part": {"type": "text", "text": "Final"}}))
        self.assertEqual(stream.text, "Final")
        self.assertTrue(stream.complete)
        self.assertEqual(stream.session, "ses_1")
        stream.feed(json.dumps({"type": "error", "error": {"message": "boom"}}))
        self.assertEqual(stream.error, "boom")

    def test_opencode_run_writes_private_config_and_discards_session(self):
        chat = Chat(dict(self.env, AI_PROVIDER="opencode"))
        chat.view("Hello")
        self.wait(chat)
        self.assertEqual(chat.action("copy-last"), "Answer ✅")
        captured = read_json(self.root / "capture.json")
        self.assertEqual(captured["args"][0], "run")
        self.assertNotIn("--continue", captured["args"])
        sessions = (self.root / "sessions.log").read_text(encoding="utf-8").strip().splitlines()
        self.assertEqual(json.loads(sessions[-1]), ["session", "delete", "ses_fake"])

    def test_stream_filters_and_no_double_append(self):
        stream = Stream("claude")
        stream.feed("not json")
        stream.feed(json.dumps({"type": "stream_event", "event": {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "Hi"}}}))
        stream.feed(json.dumps({"type": "assistant", "message": {"content": [{"type": "text", "text": "Hi"}]}}))
        stream.feed(json.dumps({"type": "result", "subtype": "success", "result": "Hi"}))
        self.assertEqual(stream.text, "Hi")
        self.assertTrue(stream.complete)

    def test_cache_eviction_recovers_without_losing_conversation(self):
        saved = fresh("codex")
        saved.update(job=saved["id"], started=time.time() - 30,
                     messages=[dict(role="user", content="Interrupted by cache eviction", status="pending")])
        self.chat.save(saved)
        result = self.chat.view()
        self.assertNotIn("rerun", result)
        self.assertIn("Background worker stopped", result["response"])
        self.assertNotIn("job", read_json(self.chat.path))


if __name__ == "__main__":
    unittest.main()
