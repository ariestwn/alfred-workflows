#!/usr/bin/env python3
"""Alfred Text View chat, using the user's signed-in Codex / Claude CLI."""
import contextlib
import fcntl
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import uuid

BUNDLE = "com.ariestwn.ai-chat"
PROVIDERS = {"codex": "Codex", "claude": "Claude", "opencode": "OpenCode"}
DEFAULT_MODELS = {"codex": "gpt-5.6-sol", "claude": "", "opencode": "deepseek/deepseek-flash"}
DEFAULT_EFFORTS = {"codex": "high", "claude": "default", "opencode": ""}
OPENCODE_AGENT = "alfred"
FOOTER = "↩ Ask · ⌘↩ New chat · ⌥↩ Copy last · ⌃↩ Copy all · ⇧↩ Stop"
SYSTEM = ("You are a helpful conversational assistant inside Alfred. Answer the latest user "
          "message using the conversation supplied as JSON. Use Markdown when useful and "
          "reply in the user's language. You have no tools in this chat. Do not claim to "
          "have accessed files, run commands, or browsed the web.")
MAX_OUTPUT = 8 * 1024 * 1024
MAX_INPUT = 100000


def read_json(path, default=None):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return default


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, name = tempfile.mkstemp(prefix=".write-", dir=str(path.parent))
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(value, handle, ensure_ascii=False)
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def integer(env, key, default, low, high):
    try:
        value = int(env.get(key) or default)
        if low <= value <= high:
            return value
    except ValueError:
        pass
    raise ValueError("{} must be between {} and {}.".format(key, low, high))


def search_path(env):
    home = Path.home()
    paths = [home / ".local/bin", home / ".opencode/bin", home / ".npm-global/bin",
             home / ".volta/bin", Path("/opt/homebrew/bin"), Path("/usr/local/bin")]
    return os.pathsep.join(map(str, paths)) + os.pathsep + env.get("PATH", os.defpath)


def configuration(provider, env):
    key = "AI_" + provider.upper()
    override = env.get(key + "_PATH", "").strip()
    executable = str(Path(override).expanduser()) if override else shutil.which(provider, path=search_path(env))
    if not executable or not Path(executable).is_file() or not os.access(executable, os.X_OK):
        raise ValueError("{} CLI not found. Install it and sign in from Terminal, or set its executable in Configure Workflow.".format(PROVIDERS[provider]))
    effort = env.get(key + "_EFFORT", DEFAULT_EFFORTS[provider]).strip()
    allowed = {"", "default", "low", "medium", "high", "xhigh", "max"}
    if provider == "codex":
        allowed.update(["none", "minimal", "ultra"])
    if effort not in allowed:
        raise ValueError("Invalid {} effort: {}".format(PROVIDERS[provider], effort))
    return dict(provider=provider, executable=str(Path(executable).absolute()),
                model=(env.get(key + "_MODEL_OVERRIDE", "").strip() or
                       env.get(key + "_MODEL", DEFAULT_MODELS[provider]).strip()), effort=effort,
                timeout=integer(env, "AI_TIMEOUT", 180, 1, 1800),
                context=integer(env, "AI_CONTEXT", 24, 2, 100),
                system=SYSTEM + "\n\n" + env.get("AI_SYSTEM_PROMPT", "").strip())


def command(config, directory):
    cmd = [config["executable"]]
    if config["provider"] == "codex":
        cmd += ["exec", "--json", "--skip-git-repo-check", "--ephemeral", "--ignore-user-config",
                "--sandbox", "read-only", "--color", "never",
                "-c", 'approval_policy="never"', "-c", "features.shell_tool=false",
                "-c", 'web_search="disabled"', "-c", "project_doc_max_bytes=0",
                "-c", "developer_instructions=" + json.dumps(config["system"], ensure_ascii=False),
                "--output-last-message", str(directory / "answer.txt")]
        if config["effort"] not in ("", "default"):
            cmd += ["-c", "model_reasoning_effort=" + json.dumps(config["effort"])]
    elif config["provider"] == "opencode":
        cmd += ["run", "--format", "json", "--agent", OPENCODE_AGENT]
    else:
        cmd += ["--print", "--output-format", "stream-json", "--verbose", "--include-partial-messages",
                "--no-session-persistence", "--tools", "", "--strict-mcp-config",
                "--mcp-config", '{"mcpServers":{}}', "--disable-slash-commands",
                "--permission-mode", "dontAsk", "--settings", '{"disableAllHooks":true}',
                "--system-prompt", config["system"]]
        if config["effort"] not in ("", "default"):
            cmd += ["--effort", config["effort"]]
    if config["model"]:
        cmd += ["--model", config["model"]]
    if config["provider"] == "codex":
        cmd.append("-")
    return cmd


class Stream:
    """Parse provider events; diagnostics and reasoning never become answer text."""
    def __init__(self, provider):
        self.provider = provider
        self.text = ""
        self.error = ""
        self.complete = False
        self.result = None
        self.session = None

    def feed(self, line):
        try:
            event = json.loads(line)
        except (ValueError, UnicodeError):
            return
        if not isinstance(event, dict):
            return
        kind = event.get("type")
        if self.provider == "codex":
            item = event.get("item") or {}
            if kind in ("item.updated", "item.completed") and item.get("type") == "agent_message":
                self.text = item.get("text", "")
            elif kind == "turn.completed":
                self.complete = True
            elif kind == "turn.failed":
                self.error = str((event.get("error") or {}).get("message", "Codex request failed."))
            elif kind == "error":
                # Codex also emits transient reconnect errors; a completed turn wins.
                self.error = str(event.get("message", "Codex request failed."))
        elif self.provider == "opencode":
            self.session = event.get("sessionID") or self.session
            if kind == "step_start":
                # Only the final step's text becomes the answer.
                self.text = ""
                self.complete = False
            elif kind == "text":
                part = event.get("part") or {}
                if part.get("type") == "text":
                    self.text += part.get("text", "")
                    self.complete = True
            elif kind == "error":
                self.error = str((event.get("error") or {}).get("message") or "OpenCode request failed.")
        else:
            if event.get("parent_tool_use_id"):
                return
            if kind == "stream_event":
                inner = event.get("event") or {}
                if inner.get("type") == "message_start":
                    self.text = ""
                delta = inner.get("delta") or {}
                if inner.get("type") == "content_block_delta" and delta.get("type") == "text_delta":
                    self.text += delta.get("text", "")
            elif kind == "assistant":
                self.text = "".join(block.get("text", "") for block in
                                    (event.get("message") or {}).get("content", [])
                                    if block.get("type") == "text")
            elif kind == "result":
                if event.get("is_error") or event.get("subtype") != "success":
                    self.error = str(event.get("result") or event.get("errors") or
                                     event.get("error") or event.get("subtype") or "Claude request failed.")
                else:
                    self.result = event.get("result", "")
                    self.text = self.result
                    self.complete = True


def stop_process(child):
    if child is None:
        return
    # The worker owns this live process group; no persisted PID is ever signalled.
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        child.wait(timeout=0.5)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait()


def opencode_settings(system):
    """A private config that denies every tool and supplies the chat instructions."""
    rule = dict(action="*", resource="*", effect="deny")
    return {"$schema": "https://opencode.ai/config.json", "permissions": [rule],
            "agents": {OPENCODE_AGENT: dict(mode="primary", description="Alfred AI Chat",
                       system=system, permissions=[rule])}}


def discard_session(config, directory, env, session):
    """Best-effort removal so chat turns do not pile up in OpenCode's own session list."""
    try:
        subprocess.run([config["executable"], "session", "delete", session], cwd=directory,
                       env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
    except (OSError, subprocess.SubprocessError):
        pass


def worker(job):
    request = read_json(job / "request.json")
    config = request["config"]
    stream = Stream(config["provider"])
    child = None
    status, detail = "error", "Request did not finish."
    started = time.monotonic()
    stdout_path, stderr_path = job / "stdout.jsonl", job / "stderr.txt"
    run_dir = job / "work"
    run_dir.mkdir(mode=0o700)
    # OpenCode treats each working directory as a project and resolves its config from PWD,
    # which Popen does not update. Use one stable project so chats do not pile up as projects.
    project = (job.parent / "opencode") if config["provider"] == "opencode" else run_dir
    project.mkdir(parents=True, exist_ok=True, mode=0o700)
    env = dict(os.environ, PATH=search_path(os.environ), NO_COLOR="1", PWD=str(project))
    state = lambda: dict(status="running", text=stream.text, heartbeat=time.time())
    write_json(job / "state.json", state())
    try:
        if (job / "cancel").exists():
            status, detail = "interrupted", "Stopped."
            return
        # File-backed stdin/output cannot deadlock when a CLI stops reading or is noisy.
        (run_dir / "prompt.txt").write_text(request["prompt"], encoding="utf-8")
        if config["provider"] == "opencode":
            write_json(project / "opencode.json", opencode_settings(config["system"]))
        with (run_dir / "prompt.txt").open("rb") as stdin, stdout_path.open("wb") as out, stderr_path.open("wb") as err:
            child = subprocess.Popen(command(config, run_dir), stdin=stdin, stdout=out, stderr=err,
                                     cwd=project, env=env, start_new_session=True)
        with stdout_path.open("rb") as output:
            pending = b""
            last_write = 0.0
            while True:
                code = child.poll()
                if any(p.exists() and p.stat().st_size > MAX_OUTPUT for p in
                       (stdout_path, stderr_path, run_dir / "answer.txt")):
                    raise ValueError("CLI output exceeded the 8 MB limit.")
                pending += output.read(MAX_OUTPUT + 1)
                lines = pending.split(b"\n")
                pending = lines.pop()
                for line in lines:
                    stream.feed(line)
                now = time.monotonic()
                if now - last_write >= 0.15:
                    write_json(job / "state.json", state())
                    last_write = now
                if (job / "cancel").exists():
                    status, detail = "interrupted", "Stopped."
                    break
                if now - started >= config["timeout"]:
                    raise ValueError("{} timed out after {} seconds. Increase Timeout in Configure Workflow or try again.".format(PROVIDERS[config["provider"]], config["timeout"]))
                if code is not None:
                    if pending:
                        stream.feed(pending)
                    diagnostics = stderr_path.read_text(encoding="utf-8", errors="replace").strip()
                    if code != 0:
                        raise ValueError(stream.error or diagnostics[-1600:] or "CLI exited with status {}.".format(code))
                    if not stream.complete:
                        raise ValueError(stream.error or "CLI exited without a completed answer. Check login and model settings.")
                    if config["provider"] == "codex":
                        final = run_dir / "answer.txt"
                        if not final.exists():
                            raise ValueError("Codex did not write a final answer.")
                        stream.text = final.read_text(encoding="utf-8")
                    if not stream.text.strip():
                        raise ValueError("CLI returned an empty answer.")
                    status, detail = "done", ""
                    break
                time.sleep(0.05)
    except Exception as error:
        detail = str(error)
    finally:
        try:
            stop_process(child)
        except OSError as error:
            status, detail = "error", "Unable to stop CLI process: " + str(error)
        if config["provider"] == "opencode" and stream.session:
            discard_session(config, project, env, stream.session)
        # Final state is the handoff: the controller may delete the job after this write.
        write_json(job / "state.json", dict(status=status, text=stream.text,
                   error=detail, heartbeat=time.time()))


def fresh(provider):
    return dict(id=str(uuid.uuid4()), provider=provider, messages=[], updated=time.time())


class Chat:
    def __init__(self, env=None):
        self.env = dict(os.environ if env is None else env)
        self.provider = self.env.get("AI_PROVIDER", "codex") or "codex"
        if self.provider not in PROVIDERS:
            raise ValueError("Provider must be " + ", ".join(sorted(PROVIDERS)) + ".")
        self.data = Path(self.env.get("alfred_workflow_data") or
                         Path.home() / "Library/Application Support/Alfred/Workflow Data" / BUNDLE)
        self.cache = Path(self.env.get("alfred_workflow_cache") or
                          Path.home() / "Library/Caches/com.runningwithcrayons.Alfred/Workflow Data" / BUNDLE)
        self.data.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.cache.mkdir(parents=True, exist_ok=True, mode=0o700)

    @property
    def path(self):
        return self.data / self.provider / "chat.json"

    @contextlib.contextmanager
    def locked(self):
        with (self.data / (self.provider + ".lock")).open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            yield

    def save(self, chat):
        chat["updated"] = time.time()
        write_json(self.path, chat)

    def archive(self, chat):
        if chat["messages"] and self.env.get("AI_KEEP_HISTORY", "1") == "1":
            write_json(self.data / "archive" / self.provider / (chat["id"] + ".json"), chat)

    def job_dir(self, chat):
        return self.cache / ("job-" + str(uuid.UUID(chat["job"])))

    def settle(self, chat, cancel=False):
        if not chat.get("job"):
            return None
        job = self.job_dir(chat)
        state = read_json(job / "state.json", {})
        if cancel:
            job.mkdir(parents=True, exist_ok=True, mode=0o700)
            (job / "cancel").touch(mode=0o600)
        if not state or (state.get("status") == "running" and time.time() - state.get("heartbeat", 0) > 15):
            if time.time() - chat.get("started", 0) <= 15:
                state = dict(status="running", text="")
            else:
                job.mkdir(parents=True, exist_ok=True, mode=0o700)
                (job / "cancel").touch(mode=0o600)
                state = dict(status="error", text=state.get("text", ""), error="Background worker stopped. Please retry.")
        if cancel and state.get("status") == "running":
            # Worker checks the cancellation marker before publishing a successful result.
            state = dict(status="interrupted", text=state.get("text", ""), error="Stopped.")
        if state["status"] == "running":
            return state
        successful = state["status"] == "done"
        chat["messages"][-1]["status"] = "complete" if successful else state["status"]
        if state.get("text"):
            chat["messages"].append(dict(role="assistant", content=state["text"],
                status="complete" if successful else state["status"]))
        chat["error"] = "" if successful else state.get("error", "Request failed.")
        chat.pop("job", None)
        chat.pop("started", None)
        self.save(chat)
        # Cancelled workers may still be shutting down; their files are cleaned on a later view.
        if not cancel and read_json(job / "state.json", {}).get("status") in ("done", "error", "interrupted"):
            shutil.rmtree(job, ignore_errors=True)
        return None

    def cleanup(self):
        for path in self.cache.glob("job-*"):
            if path.is_dir() and time.time() - path.stat().st_mtime > 86400:
                state = read_json(path / "state.json", {})
                if state.get("status") != "running" or time.time() - state.get("heartbeat", 0) > 3600:
                    shutil.rmtree(path, ignore_errors=True)

    def start(self, chat, query):
        if len(query) > MAX_INPUT:
            raise ValueError("A message can contain at most 100,000 characters.")
        config = configuration(self.provider, self.env)
        history = [dict(role=m["role"], content=m["content"]) for m in chat["messages"]
                   if m.get("status", "complete") == "complete"][-config["context"]:]
        while history and history[0]["role"] != "user":
            history.pop(0)
        history.append(dict(role="user", content=query))
        prompt = "Reply to the latest user message in this conversation (JSON):\n" + json.dumps(history, ensure_ascii=False)
        if len(prompt.encode("utf-8")) > 2 * 1024 * 1024:
            raise ValueError("Conversation context is too large. Reduce Context or start a new chat.")
        ident = str(uuid.uuid4())
        job = self.cache / ("job-" + ident)
        write_json(job / "request.json", dict(config=config, prompt=prompt))
        write_json(job / "state.json", dict(status="running", text="", heartbeat=time.time()))
        try:
            with open(os.devnull, "rb") as stdin, open(os.devnull, "wb") as output:
                child = subprocess.Popen([sys.executable, "-B", str(Path(__file__).resolve()), "worker", str(job)],
                                 env=self.env, stdin=stdin, stdout=output, stderr=output,
                                 start_new_session=True, close_fds=True)
                # Reap the worker in long-lived callers (tests); Alfred's short-lived
                # view process exits immediately and the OS adopts the detached child.
                threading.Thread(target=child.wait, daemon=True).start()
        except Exception:
            shutil.rmtree(job, ignore_errors=True)
            raise
        chat["messages"].append(dict(role="user", content=query, status="pending"))
        chat.update(job=ident, started=time.time(), error="")
        self.save(chat)

    def view(self, query=""):
        with self.locked():
            self.cleanup()
            chat = read_json(self.path, fresh(self.provider))
            state = self.settle(chat)
            footer = FOOTER
            if query.strip() and self.env.get("AI_POLL") != "1":
                if state:
                    footer = "Still answering. Stop with ⇧↩ before sending another question."
                else:
                    try:
                        self.start(chat, query)
                        state = dict(status="running", text="")
                    except (ValueError, OSError) as error:
                        return self.render(chat, error=str(error), select=True)
            return self.render(chat, state=state, footer=footer, select=bool(state and query.strip() and footer != FOOTER))

    def render(self, chat, state=None, error="", footer=FOOTER, select=False):
        response = markdown(chat)
        if not response:
            response = "# {}\n\nAsk a question to start a conversation.".format(PROVIDERS[self.provider])
        if state:
            response += "\n\n" + (state.get("text") or "Thinking…")
        detail = error or chat.get("error", "")
        if detail:
            response += "\n\n---\n\n" + detail
        result = dict(response=response, footer=footer,
                      variables={"AI_POLL": "1" if state else ""},
                      behaviour=dict(response="replace", scroll="end", inputfield="select" if select else "clear"))
        if state:
            result["rerun"] = 0.3
        return result

    def action(self, action, query=""):
        with self.locked():
            chat = read_json(self.path, fresh(self.provider))
            self.settle(chat, cancel=action in ("new", "stop", "restore"))
            if action == "new":
                self.archive(chat)
                self.save(fresh(self.provider))
            elif action == "restore":
                ident = str(uuid.UUID(query))
                saved = read_json(self.data / "archive" / self.provider / (ident + ".json"))
                if not saved or saved.get("provider") != self.provider:
                    raise ValueError("Chat history not found.")
                self.archive(chat)
                saved.pop("job", None)
                self.save(saved)
                query = ""
            elif action == "copy-last":
                return next((m["content"] for m in reversed(chat["messages"])
                             if m["role"] == "assistant" and m.get("status") == "complete"), "")
            elif action == "copy-all":
                return markdown(chat)
            elif action == "stop":
                query = ""
            else:
                raise ValueError("Unknown action.")
            return {"alfredworkflow": {"arg": query, "variables": {"AI_POLL": "", "AI_PROVIDER": self.provider}}}

    def history(self):
        entries = []
        for provider in PROVIDERS:
            for path in (self.data / "archive" / provider).glob("*.json"):
                try:
                    chat = read_json(path)
                    questions = [m["content"] for m in chat["messages"] if m["role"] == "user"]
                    if questions:
                        entries.append((chat.get("updated", 0), dict(uid=provider + path.stem,
                            title=questions[0].replace("\n", " ")[:160],
                            subtitle=PROVIDERS[provider] + " · " + questions[-1].replace("\n", " ")[:160],
                            match="{} {} {}".format(provider, questions[0], questions[-1]),
                            arg=path.stem, variables={"AI_PROVIDER": provider}, valid=True)))
                except (ValueError, KeyError, TypeError):
                    continue
        return dict(items=[item for _, item in sorted(entries, key=lambda e: e[0], reverse=True)] or
                    [dict(title="No chat history yet", subtitle="Start a new chat with ⌘↩ to archive the current conversation.", valid=False)])


def markdown(chat):
    parts = []
    for message in chat["messages"]:
        if message["role"] == "user":
            parts.append("## You\n\n" + message["content"] + "\n\n## " + PROVIDERS[chat["provider"]])
        else:
            parts.append(message["content"])
        if message.get("status") in ("interrupted", "error"):
            parts.append("[Answer interrupted]" if message["status"] == "interrupted" else "[Request failed]")
    return "\n\n".join(parts)


def main(argv):
    os.umask(0o077)
    action = argv[0] if argv else "view"
    query = argv[1] if len(argv) > 1 else ""
    if action == "worker":
        worker(Path(query))
        return
    try:
        chat = Chat()
        if action == "view":
            result = chat.view(query)
        elif action == "history":
            result = chat.history()
        else:
            result = chat.action(action, query)
        if isinstance(result, str):
            sys.stdout.write(result)
        else:
            print(json.dumps(result, ensure_ascii=False))
    except Exception as error:
        if action == "view":
            print(json.dumps(dict(response="Unable to open chat: " + str(error), footer="Check Configure Workflow or the workflow debugger.")))
        elif action == "history":
            print(json.dumps(dict(items=[dict(title="Unable to read history", subtitle=str(error), valid=False)])))
        else:
            print(str(error), file=sys.stderr)
            sys.exit(1)


if __name__ == "__main__":
    main(sys.argv[1:])
