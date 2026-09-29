# QuickAI for Alfred

A Rust port of the writing tools in `raycast/quickai`, with exactly three editing commands. Uses your locally installed, signed-in Codex, Claude Code, or OpenCode CLI.

| Keyword / action | Behavior |
| --- | --- |
| `quickfix` | Fix typos, grammar, and obvious awkward phrasing with minimal edits. |
| `improve-writing` | Rephrase casually in lowercase, like talking to a friend, while preserving meaning. |
| `grammar-check` | Correct spelling, punctuation, and grammar without stylistic rewriting. |

Grammar Check returns corrected text, not a report. The original language and formatting are preserved. Quickfix is new; the built-in Improve Writing and Grammar Check prompts are ported from the Raycast extension. Alfred's Improve Writing prompt override defaults to the casual style below.

## Install

1. Install Alfred 5 with Powerpack and a current Codex CLI, Claude Code CLI, and/or OpenCode CLI. Sign in to your chosen CLI in Terminal first (`codex login`, `claude`, or `opencode auth login`).
2. Download the Apple Silicon or Intel build from the [releases page](https://github.com/ariestwn/alfred-workflows/releases) and open it to import into Alfred.
3. Open **Configure Workflow…**. Select **Provider**, then set that provider's **model** and (for Codex and Claude) **effort**.
4. Enable Alfred under **System Settings → Privacy & Security → Accessibility** for selection reading and pasting.

Each build contains a native binary for one architecture only. Rust and Python are not required to run the packaged workflow.

## Use

- Select text, open Alfred, and run one of the three keywords.
- Or select text and use Alfred's **Universal Actions** shortcut, then choose **QuickAI · Quickfix**, **QuickAI · Improve Writing**, or **QuickAI · Grammar Check**.
- Or enter text directly, for example `grammar-check She dont like apples.`
- Optional: double-click one of the three unassigned Hotkey objects in the workflow editor and assign a shortcut. These pass the selected text to the same commands.
- Or open a deeplink from Raycast, Shortcuts, a Stream Deck, or Terminal. Without an argument, the deeplink rewrites the current selection. With `?argument=`, it rewrites the URL-encoded text instead:

  ```sh
  open -g 'alfred://runtrigger/com.ariestwn.quickai/quickfix/'
  open -g 'alfred://runtrigger/com.ariestwn.quickai/improve-writing/'
  open -g 'alfred://runtrigger/com.ariestwn.quickai/grammar-check/?argument=She%20dont%20like%20apples.'
  ```

  `-g` keeps the current app in front so the result can be pasted back into it.

The default output is **Paste (replace selection)**. **Copy to clipboard** and **Paste and copy** are also available. Paste uses Alfred's transient clipboard option; Paste and copy keeps the result in clipboard history. If the focused app or selected text changed while the model was running, QuickAI copies the result and tells you. Stay in the original field until the request completes if you want automatic replacement. Typed input is copied when it does not match the current selection.

Keywords capture text using Alfred's native **⌘C** action, including in browsers such as Dia and editors that do not expose `AXSelectedText`. QuickAI waits for a fresh clipboard update, reads the selected text, and restores all formats of your previous clipboard before requesting the rewrite. It repeats that check before pasting to confirm that the original text is still selected. Stale clipboard contents are never used as the selection. Applications must support Copy; Alfred needs Accessibility permission to send the shortcut. Clipboard snapshots are private temporary files, removed after capture; abandoned snapshots expire after five minutes.

Errors produce a status message and never enter the copy/paste branch. Text is limited to 16,000 Unicode characters. Requests time out after 120 seconds by default.

QuickAI shows a compact floating status popup near the bottom of the screen while it works. Progress and success use a single line with 15px vertical and 20px horizontal padding; the popup sizes to its text. Errors include a wrapping explanation. A rounded alpha mask clips both the background blur and window shadow. The popup updates after the copy/paste action, then disappears after three seconds (seven seconds for errors). It never becomes the active window, accepts keyboard input, or intercepts clicks. There is no Dock icon or permanent background service. Simultaneous requests use separate popups. The native macOS appearance and Reduce Motion setting are respected, and status changes are announced to VoiceOver.

Choose **Status messages → macOS notification** for the original Notification Center behavior, or **Off** to suppress status messages. If the popup cannot start, QuickAI falls back to a macOS notification. The popup reports that a paste was sent; it cannot confirm that the destination app accepted the paste. An interrupted request dismisses its popup automatically. Private temporary status files contain only status messages and are removed when the popup closes.

When its message changes, the HUD grows or shrinks around its center over 220ms with a smooth easing curve. Text fades in without scaling, and rounded corners stay clipped throughout. A new status can retarget an unfinished resize. macOS **Reduce Motion** disables resizing animations and text fades. This uses native AppKit animation from Rust.

## Configuration

All settings are available in Alfred's **Configure Workflow…** panel. Each provider keeps its own model and effort, so switching providers doesn't overwrite the other provider's settings.

| Variable | Default | Meaning |
| --- | --- | --- |
| `QUICKAI_PROVIDER` | `codex` | `codex`, `claude`, or `opencode` |
| `QUICKAI_CODEX_MODEL` | `gpt-5.6-luna` in Alfred | Codex model ID; empty uses the CLI's built-in default |
| `QUICKAI_CODEX_EFFORT` | `medium` in Alfred | `default`, `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, `ultra` |
| `QUICKAI_CLAUDE_MODEL` | empty | Claude model ID or alias, e.g. `sonnet`; empty uses the CLI default |
| `QUICKAI_CLAUDE_EFFORT` | `low` in Alfred | `default`, `low`, `medium`, `high`, `xhigh`, `max` |
| `QUICKAI_OPENCODE_MODEL` | `deepseek/deepseek-flash` in Alfred | OpenCode model ID as `provider/model`, e.g. `deepseek/deepseek-flash`; empty uses the CLI default. Append `#variant` for a model variant |
| `QUICKAI_OUTPUT` | `paste` | `paste`, `copy`, or `both` |
| `QUICKAI_TIMEOUT` | `120` | Timeout in seconds, from 1 to 600 |
| `QUICKAI_FEEDBACK` | `hud` | `hud` (floating popup), `notification` (macOS), or `off` |
| `QUICKAI_CODEX_PATH` | empty | Optional CLI executable path, including paths with spaces |
| `QUICKAI_CLAUDE_PATH` | empty | Optional CLI executable path |
| `QUICKAI_OPENCODE_PATH` | empty | Optional CLI executable path |
| `QUICKAI_OPENCODE_DIR` | empty | Advanced: stable project directory for OpenCode. Empty uses `~/Library/Caches/com.ariestwn.quickai/opencode` |
| `QUICKAI_GLOBAL_PROMPT` | empty | Extra writing rules applied to every command |
| `QUICKAI_QUICKFIX_PROMPT` | empty | Override Quickfix's editing instruction |
| `QUICKAI_IMPROVE_PROMPT` | Casual prompt below in Alfred | Override Improve Writing's editing instruction; clear to restore the built-in prompt |
| `QUICKAI_GRAMMAR_PROMPT` | empty | Override Grammar Check's editing instruction |

Improve Writing uses this prompt by default in Alfred:

> Tone: Conversational, like talking to a friend.  - Use natural connectors when possible . - Slightly longer than punchy is fine, but still tight. No filler. - Should sound spoken, not written. all lowercase and rephrase into casual.

Use only effort levels supported by your selected model and installed CLI. Unsupported combinations produce the provider's error; the workflow doesn't silently change your model or effort. `default` omits the effort override. When running the binary directly, an unset effort also uses the CLI default. OpenCode has no effort setting; append a model variant to its model ID instead, for example `deepseek/deepseek-flash#high`.

Executable discovery includes `~/.local/bin`, `~/.opencode/bin`, `~/.npm-global/bin`, `~/.volta/bin`, Homebrew, and the inherited PATH. If Node or a CLI is installed through a version manager, supply an executable wrapper that sets its PATH, or use a native CLI installation. Shell aliases and shell startup files are not loaded.

## Provider behavior

- Codex runs `codex exec` with `--output-last-message`, stdin input, ephemeral sessions, read-only sandboxing, and no approval prompts. It ignores user config, disables the shell tool and web search, and skips project instruction discovery. Saved authentication is reused; set any desired model in this workflow rather than relying on `config.toml`.
- Claude runs `claude --print --output-format json`, the noninteractive equivalent of an exec command. Built-in tools, MCP servers, hooks, and slash commands are disabled for the request. Session persistence is disabled. Only a successful final `result` is accepted.
- OpenCode runs `opencode run --format json --agent quickai` with a private `opencode.json` written to a stable project directory in the cache. That config denies every tool and supplies the editing instructions, so the model rewrites the text without running commands. Saved OpenCode credentials are reused, and the short-lived session is deleted after the request so edited text does not linger in OpenCode's history.
- Requests run with their input, output, and diagnostics in a unique private temporary directory that is removed afterwards. OpenCode instead runs from its stable cache project so it does not create a new project per request. Prompts are passed over stdin and command arguments are never shell-interpolated. Diagnostic streams cannot become rewritten text. A timeout terminates the CLI process group, including descendants.

The selected text is sent through your chosen CLI to its provider. Its account access, usage limits, and billing apply. No API key is stored in the workflow.

References: [Codex noninteractive mode](https://developers.openai.com/codex/noninteractive/), [Claude CLI reference](https://code.claude.com/docs/en/cli-reference), [OpenCode CLI](https://opencode.ai/v2/docs/cli), [Alfred configuration](https://www.alfredapp.com/help/workflows/workflow-configuration/).

## Build and verify

Install stable Rust and Apple's Command Line Tools, then:

```sh
./scripts/cargo.sh test --locked
./build.sh
/usr/bin/python3 scripts/verify_workflow.py
```

`scripts/cargo.sh` uses the project-local toolchain in `.tools/` when present, otherwise the system Cargo. `build.sh` compiles an optimized binary, signs it ad hoc on macOS, generates `workflow/info.plist`, and creates `dist/QuickAI.alfredworkflow`. Python is used only for build metadata and archive verification; all runtime logic is Rust, with a one-line Alfred shell launcher.

Smoke-test the real CLI without interacting with the clipboard:

```sh
QUICKAI_PROVIDER=codex QUICKAI_CODEX_MODEL=gpt-5.6-luna QUICKAI_CODEX_EFFORT=medium \
  ./workflow/quickai grammar-check -- 'She dont like apples.'

QUICKAI_PROVIDER=claude QUICKAI_CLAUDE_MODEL=sonnet QUICKAI_CLAUDE_EFFORT=low \
  ./workflow/quickai improve-writing -- 'We was hoping to get this done soon.'

QUICKAI_PROVIDER=opencode QUICKAI_OPENCODE_MODEL=deepseek/deepseek-flash \
  ./workflow/quickai quickfix -- 'teh quick brown fox'

printf '%s' 'aku sudah mengirim pesan nya.' | \
  ./workflow/quickai quickfix --stdin
```

Tests cover provider selection, literal arguments, Unicode limits, whitespace and markdown preservation, JSON errors, absent final answers, timeout/descendant cleanup, output routing, and paste destination changes. Mock CLI tests do not use your account or make network requests.

To check the native popup on a logged-in Mac, run `./scripts/cargo.sh build --locked` followed by `python3 scripts/smoke_hud.py`. This checks foreground app ownership, working/success/error status, interruption cleanup, the Alfred process handoff with a mock provider, and notification settings. It shows temporary sample popups but never types, clicks, modifies the clipboard, or calls an AI provider. Debug builds render only the popup's own view to temporary PNGs for inspection; this rendering hook is excluded from release builds.

## Troubleshooting

- **Cannot start CLI:** set the executable path in Configure Workflow. Verify the CLI runs in Terminal.
- **Authentication or model error:** sign in to that CLI in Terminal and check the model/effort combination. Recent CLI versions are required for the flags above.
- **No text was copied:** select text in the source app and make sure ⌘C works there. Enable Accessibility for Alfred if its Copy shortcut is blocked. Existing clipboard contents are not used as fallback input.
- **Result copied instead of pasted:** the original app/selection couldn't be confirmed. Paste manually with ⌘V.
- **Timeout:** choose a faster model/lower effort or increase Timeout.
- **More details:** use Alfred's workflow debugger or run the binary from Terminal.
- **Popup unavailable:** QuickAI uses a macOS notification automatically. Status popups run only for Alfred workflow invocations; normal terminal commands keep using stdout/stderr.
