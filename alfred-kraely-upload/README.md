# Upload to Remote for Alfred

A Rust port of the Raycast `screenshot-to-kraely` uploader. One Universal Action
uploads the selected file to a VPS over SSH, then copies its absolute remote path.

## Use

1. Download the Apple Silicon or Intel build from the [releases page](https://github.com/ariestwn/alfred-workflows/releases) and open it in Alfred 5 with Powerpack.
2. Open **Configure Workflow**, enter your **SSH host** (an alias from `~/.ssh/config` or `user@hostname`), and set the remote folder.
3. Select one file in Finder (or select a file result in Alfred).
4. Open Alfred Universal Actions and choose **Upload to Remote**.
5. When the popup says **Remote path copied**, paste with ⌘V.

For `example.png`, the default result is `/home/ubuntu/screenshot/example.png`.
The copied value is the server's absolute filesystem path. It is not a public URL.
This action accepts one regular file of any type, including screenshots and PDFs.

## External Trigger

Other workflows, scripts, and automations can start the same upload through the
`upload` External Trigger. Pass one absolute file path as the argument; `~` is not
expanded. The result is identical to the Universal Action, including the popup
and clipboard copy.

```sh
osascript -e 'tell application id "com.runningwithcrayons.Alfred" to run trigger "upload" in workflow "com.ariestwn.kraely-upload" with argument "/Users/me/Desktop/example.png"'
```

From another Alfred workflow, add a **Call External Trigger** output with
workflow `com.ariestwn.kraely-upload` and trigger `upload`. The `alfred://` URL
handler is off, so web pages cannot start uploads of local files.

## Configuration

| Setting | Default |
| --- | --- |
| SSH host | Empty; required before the first upload |
| Remote folder | `/home/ubuntu/screenshot/` |
| Maximum file size | 20 MB |
| Upload timeout | 60 seconds |
| Status messages | Floating popup |

The SSH alias must exist in `~/.ssh/config`, or use `user@hostname`. Configure
ports, identity files, and jump hosts there. The remote folder must already exist
and be writable. Files retain their names, including spaces and Unicode; an
existing remote file with the same name is overwritten, as in the Raycast version.

Requires macOS 14 or later with Apple's OpenSSH 9+ (SFTP-based `scp`), and SSH
key authentication that works without a password prompt. Connect from Terminal
first to verify the host key and make sure the key is available to your SSH agent:

```sh
ssh my-server
```

The workflow uses your existing SSH settings and agent. It does not store keys,
passwords, or server credentials. Unknown or changed host keys fail verification.

The compact native HUD shows upload progress and success/errors without taking
focus. It keeps 15 pt vertical / 20 pt horizontal padding, transparent rounded
corners, smooth width changes, and honors Reduce Motion. Status messages can also
use macOS notifications or be disabled. Clipboard contents change only after
`scp` reports success; failures and timeouts do not copy an empty or invalid path.
Like `scp`, an interrupted upload may leave a partial remote file; retry to replace it.

## Build and verify

```sh
./scripts/cargo.sh generate-lockfile --offline
./scripts/cargo.sh test --locked
./scripts/cargo.sh clippy --all-targets --locked -- -D warnings
./build.sh
python3 scripts/verify_workflow.py
```

Requires Rust 1.85+ and Xcode Command Line Tools to build. The Cargo wrapper uses
system Cargo or a project-local toolchain, including one in the sibling
`alfred-quickai` project. Alfred only needs the packaged native executable; no Rust,
Node.js, Raycast, or Swift installation is needed at runtime.

The native HUD smoke test shows temporary sample popups on a logged-in macOS
desktop, without typing, writing the clipboard, or uploading anything:

```sh
./scripts/cargo.sh build --locked
python3 scripts/smoke_upload.py
python3 scripts/smoke_hud.py
```

CLI usage (prints the path to stdout; Alfred handles the clipboard):

```sh
KRAELY_HOST=my-server KRAELY_REMOTE_DIR=/home/ubuntu/screenshot/ \
  ./workflow/kraely-upload -- /absolute/path/example.png
```

All workflow settings map to environment variables: `KRAELY_HOST`,
`KRAELY_REMOTE_DIR`, `KRAELY_MAX_SIZE_MB`, `KRAELY_TIMEOUT`, and `KRAELY_FEEDBACK`
(`hud`, `notification`, or `off`). Debug builds alone support `KRAELY_TEST_SCP`
for mock integration tests; release builds always call `/usr/bin/scp`.
`KRAELY_HOST` is required and accepts any SSH alias or `user@hostname`; there is
no default host.

The upload smoke test connects directly to Apple's local SFTP server using
[`scp -D`](https://man.openbsd.org/scp.1#D), so it exercises the real protocol
with temporary fixtures without contacting a VPS.

`src/hud.rs` and `src/hud_mac.rs` reuse the native Rust/AppKit HUD from QuickAI.
