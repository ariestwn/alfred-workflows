# AI Chat for Alfred

Chat with Codex, Claude Code, or OpenCode in Alfred's native Text View. This is a new workflow inspired by Vítor Galvão's [ChatGPT / DALL-E workflow](https://github.com/alfredapp/openai-workflow/), with a rewritten CLI backend and a separate bundle ID. It does not require an OpenAI API key in Alfred.

## Install and use

Requires Alfred 5.5+ with Powerpack, Python 3.9+ at `/usr/bin/python3` (Apple Command Line Tools), and a recent Codex CLI, Claude Code CLI, and/or OpenCode CLI. This Mac has Python 3.9.6, Codex 0.150.1, Claude Code 2.1.247, and OpenCode 2.0.1.

1. Sign in to the CLI in Terminal: `codex login`, `claude`, or `opencode auth login`.
2. Download `AI-Chat-<version>.alfredworkflow` from the [releases page](https://github.com/ariestwn/alfred-workflows/releases) and open it to import (it has no native code, so the same file works on Apple Silicon and Intel), then open **Configure Workflow…**. Codex defaults to **GPT-5.6 Sol / High**; OpenCode defaults to **DeepSeek V4.1 Flash**.
3. Run `codex your question`, `claude your question`, or `opencode your question`. `ai your question` uses the configured default provider.

| Input | Action |
| --- | --- |
| `ai`, `codex`, `claude`, `opencode` without a question | Open that provider's current conversation; reconnect to an active answer. |
| `aihistory` | Browse and resume archived conversations from any provider. |
| Keyword + ⌘↩ | Archive the current conversation and start a new one with the typed question. |
| Keyword + ⌥↩ | Browse chat history. |
| Keyword + fn↩ | Open a multiline editor, prefilled with the typed text. ↩ sends it to that provider's current conversation; ⇧↩ or ⌥↩ inserts a new line. |
| Chat: ↩ | Send the next question. |
| Chat: ⌘↩ | Start a new conversation; text in the input field becomes its first question. |
| Chat: ⌥↩ | Copy the last completed answer. |
| Chat: ⌃↩ | Copy the full conversation as Markdown. |
| Chat: fn↩ | Open the multiline editor with the text in the input field. |
| Chat: ⇧↩ | Stop the current answer. |

Universal Actions **Ask Codex**, **Ask Claude**, and **Ask OpenCode** start a new conversation with selected text. Three unassigned Hotkey objects can be configured for the same action. External triggers `ask_ai` (default provider), `ask_codex`, `ask_claude`, and `ask_opencode` send a question to that provider's current conversation, for example `osascript -e 'tell application id "com.runningwithcrayons.Alfred" to run trigger "ask_ai" in workflow "com.ariestwn.ai-chat" with argument "your question"'`.

Codex, Claude, and OpenCode keep separate current conversations. Closing Alfred does not cancel an answer. Reopening the same provider displays its progress or completed answer. Only one answer per provider runs at a time. Start a new chat or stop the current answer before sending another question while it is running.

Claude streams text as it arrives. Codex's and OpenCode's event interfaces report completed message items, so their answer may appear all at once. The workflow shows progress while waiting. Failed or interrupted exchanges stay visible locally but are excluded from future context. Copy last uses the latest successfully completed answer.

## Configuration

All settings are in **Configure Workflow…**. Model and effort are independent dropdowns for Codex and Claude; OpenCode has a model dropdown only, because DeepSeek models have no reasoning variants. Codex offers Sol, Luna, Terra, Astra, GPT-5.5, and CLI default; Claude offers Sonnet, Opus, Haiku, and CLI default; OpenCode offers DeepSeek V4.1 Flash, DeepSeek V4 Flash, DeepSeek V4 Pro, and CLI default. An optional custom model field accepts any exact model ID and takes precedence over its dropdown. For OpenCode, a custom model may also include a variant, for example `deepseek/deepseek-v4-flash#high`.

| Setting / variable | Default | Meaning |
| --- | --- | --- |
| `AI_PROVIDER` | `codex` | Provider for the `ai` keyword. |
| `AI_KEYWORD`, `AI_CODEX_KEYWORD`, `AI_CLAUDE_KEYWORD`, `AI_OPENCODE_KEYWORD` | `ai`, `codex`, `claude`, `opencode` | Chat keywords. |
| `AI_HISTORY_KEYWORD` | `aihistory` | History keyword. |
| `AI_CODEX_MODEL` | `gpt-5.6-sol` | Codex model selection. |
| `AI_CODEX_EFFORT` | `high` | Codex reasoning effort. |
| `AI_CLAUDE_MODEL`, `AI_CLAUDE_EFFORT` | CLI default | Claude model and effort selection. |
| `AI_OPENCODE_MODEL` | `deepseek/deepseek-flash` | OpenCode model selection (`provider/model`). |
| `AI_CODEX_MODEL_OVERRIDE`, `AI_CLAUDE_MODEL_OVERRIDE`, `AI_OPENCODE_MODEL_OVERRIDE` | empty | Optional exact model ID, overriding that provider's dropdown. |
| `AI_CONTEXT` | `24` | Maximum prior messages sent with each question, 2–100. Incomplete exchanges are excluded and context begins with a user message. |
| `AI_TIMEOUT` | `180` | Total request timeout in seconds, 1–1800. |
| `AI_KEEP_HISTORY` | enabled | Archive conversations when starting a new chat. Disabling this does not remove existing archives. |
| `AI_SYSTEM_PROMPT` | empty | Additional instructions for all providers. |
| `AI_CODEX_PATH`, `AI_CLAUDE_PATH`, `AI_OPENCODE_PATH` | empty | Optional absolute executable path, including paths with spaces. |

CLI discovery includes `~/.local/bin`, `~/.opencode/bin`, `~/.npm-global/bin`, `~/.volta/bin`, Homebrew, and inherited PATH. Shell aliases and startup files are not loaded. For a Node version manager, set an executable wrapper that configures PATH, or use a native CLI installation.

## Provider behavior and data

This ports the original workflow's **text chat** features. Image generation through DALL-E is not included.

- Codex uses `codex exec --json --output-last-message`, ephemeral sessions, a read-only sandbox, and no approval prompts. User config, shell tools, web search, and project instructions are disabled for chat. Saved CLI authentication is reused. Since user config is ignored, set a custom model in this workflow rather than relying on `config.toml`.
- Claude uses `claude --print --output-format stream-json --verbose --include-partial-messages`, the noninteractive equivalent of an exec command. Tools, MCP servers, hooks, and slash commands are disabled for chat; sessions are not persisted by Claude. It uses normal CLI authentication, including subscription login.
- OpenCode uses `opencode run --format json --agent alfred`. A private `opencode.json` in a small, stable project directory under the workflow cache denies every tool and supplies the chat instructions, so the model answers in text without running commands and chats do not create a new OpenCode project per message. Saved OpenCode credentials are reused, and each short-lived session is deleted after the answer so it does not build up in OpenCode's own session list.
- Requests run in a private working directory. Conversation JSON goes through stdin, without shell interpolation. Provider diagnostics never become a successful answer. Timeout and stop terminate the CLI process group, including child processes.
- Context is replayed as a transcript on every request; this is not a continuation of terminal CLI sessions. Your chosen provider's account access, usage limits, and billing apply.
- Current conversations and archives are stored under Alfred's Workflow Data directory for `com.ariestwn.ai-chat`. Temporary CLI output lives under the matching cache directory. Completed job files are removed when Alfred collects the result. Abandoned job files are removed after a day when opening a chat. No credentials are copied into the workflow or history.
- Messages are limited to 100,000 characters, request context to 2 MB, and each CLI output stream to 8 MB.

## Build and verify

No pip packages are required.

```sh
/usr/bin/python3 -B -m unittest discover -s tests -v
/usr/bin/python3 -B scripts/package.py
/usr/bin/python3 -B scripts/verify_workflow.py
```

The package contains only runtime scripts, metadata, icon, README, and license. It does not contain API keys, CLI credentials, conversations, or test fixtures.

Tests use fake executables and do not access your account. For a real smoke test with isolated data and a short fixed prompt:

```sh
/usr/bin/python3 -B scripts/smoke_cli.py codex
/usr/bin/python3 -B scripts/smoke_cli.py claude
/usr/bin/python3 -B scripts/smoke_cli.py opencode
```

If Alfred reports a missing CLI or login error, run that CLI in Terminal and check **Configure Workflow…**. If `/usr/bin/python3` is unavailable, install Apple's Command Line Tools with `xcode-select --install`. Unsupported models or efforts are reported directly; they are not silently replaced.

References: [Codex noninteractive mode](https://developers.openai.com/codex/noninteractive/), [Claude programmatic use](https://code.claude.com/docs/en/headless), [OpenCode CLI](https://opencode.ai/v2/docs/cli), [Alfred Text View JSON](https://www.alfredapp.com/help/workflows/user-interface/text/json/).
