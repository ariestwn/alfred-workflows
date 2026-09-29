# Handy Rust for Alfred

A Rust port of the Handy Raycast extension by mattiacolombomc and kud. Control Handy speech-to-text, browse history, maintain custom words, and select downloaded models and languages from Alfred.

## Install

Download the Apple Silicon or Intel build from the [releases page](https://github.com/ariestwn/alfred-workflows/releases) and open it to import into Alfred. Requires Alfred 5.5+ with Powerpack, macOS, and Handy already installed and configured. Each build contains a native executable for one architecture only.

Type `handy` to see all commands. Workflow Configuration lets you change the main keyword, Handy executable, data directory, Hugging Face cache, and history page size. An unassigned recording hotkey is also included; set it in the workflow editor if wanted.

| Keyword | Action |
| --- | --- |
| `handy` | All Handy commands |
| `hrec` | Start/stop recording |
| `hcopy` | Copy latest transcript |
| `hpaste` | Paste latest transcript into the active app |
| `hhistory [query]` | Browse/search transcript history |
| `hsaved [query]` | Browse saved transcripts |
| `hmodel [query]` | Select a downloaded model |
| `hlang [query]` | Select a supported language |
| `hword <word or phrase>` | Add a dictionary entry |
| `hdict [query]` | Browse/add/remove dictionary entries |
| `hfolder` | Open recordings in Finder |
| `hcancel` | Cancel Handy's current operation |

The same sections work under the main keyword: `handy history`, `handy models`, `handy languages`, `handy dictionary`, `handy add`, and `handy saved`.

## Transcripts

History is newest first and searches titles, original text, and post-processed text. Search terms match literally; all words must match. SQLite's built-in case folding is ASCII-based. Search runs across the database before returning a page, so older matches remain accessible. Use Next/Previous to browse further.

- Return: copy the transcript, preferring post-processed text when present.
- Command-C also copies the full displayed transcript directly from the result list.
- Command-Return: paste into the previously active app.
- Option-Return: reveal the recording in Finder.
- Control-Return: open transcript actions, including full-text view, original text, save/unsave, and delete.

Deleting requires choosing **Delete transcript…**, then **Delete transcript and recording**. The workflow first creates a private `Handy Transcript <id> …` folder in your Mac's Trash with `transcript.json` and any available audio. Only then does it remove the history row and original audio. A missing audio file does not prevent recovering the text. Transcript JSON preserves every history column and can be used for manual recovery; there is no automatic restore command. Empty or failed lookups never change the clipboard.

## Settings and recording

Model, language, and dictionary changes explicitly reopen Handy. Finish any recording before selecting one. The workflow validates the change, requests a normal quit, waits for exit, reads the latest settings, updates only the requested fields, writes atomically, and reopens Handy. It never force-quits Handy. If Handy is already closed, it saves and opens the app. A model change resets language to Auto to avoid retaining an unsupported language. Choosing the active value makes no changes.

macOS may ask to allow Alfred to control Handy for the quit request. If that is denied, quit Handy manually and retry. Automatic paste needs Alfred's normal Accessibility permission. Recording uses Handy's own microphone permissions. If Handy is closed, the recording command opens it; run the command again once it is ready.

The model list discovers actual local files: Hugging Face snapshots (including cache symlinks), legacy Handy model files, and custom models. Partial downloads and broken symlinks are excluded. It does not download models. Language support comes from the catalog shipped with the source extension; unknown custom models show all languages. Download new models in Handy itself.

The workflow reads only local Handy data and runs no network requests. It does not include an AI chat client, API keys, or transcription engine; Handy performs transcription. Clipboard history follows your Alfred settings.

## Build and validate

```sh
./scripts/cargo.sh test --locked --offline
./scripts/cargo.sh clippy --locked --offline --all-targets -- -D warnings
./build.sh
```

Requires Rust, Xcode command line tools, and Python 3 for packaging. The build uses `cargo` from PATH, falling back to the sibling `alfred-quickai/.tools` toolchain in this workspace. On a fresh machine, run `cargo fetch --locked` once before the offline build. Installed workflows do not need Rust, Python, Node, or Raycast. The binary links to macOS system SQLite and uses macOS's `open`, `ps`, and `osascript` for application lifecycle; clipboard, paste, text view, and Finder actions are native Alfred objects.

Tests use temporary databases, settings, model caches, audio, and Trash folders. They do not record audio or change your running Handy app. `scripts/benchmark.py` benchmarks search against an isolated synthetic history. `scripts/verify_workflow.py` checks package contents, object connections, routing, executable permissions, and read-only filter output.

## References

- [Original Raycast extension](https://github.com/mattiacolombomc/raycast-handy)
- [Handy command-line controls](https://handy.computer/docs/cli)
- [Alfred Script Filter JSON](https://www.alfredapp.com/help/workflows/inputs/script-filter/json/)
- [Alfred Text View](https://www.alfredapp.com/help/workflows/user-interface/text/)

Bundled catalog, language names, legacy model metadata, and icon are from the supplied MIT-licensed extension. See `THIRD_PARTY_NOTICES.md`.
