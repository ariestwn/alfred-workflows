# Verification

Verified on September 14, 2026, on Apple Silicon.

## Completed-result actions — 0.4.1

- The completed report previously disabled Return on every row and advertised Option-Return on a summary with no modifier action. The summary now opens Trash with Return or Option-Return; individual file results reveal the recorded destination with either shortcut.
- Reveal targets use the verified Trash path, including renamed destinations. Remaining files reveal their original locations. Missing destinations and entries removed by Homebrew explicitly say “No file to reveal.” Selection and Actions modifiers are disabled after execution.
- 42 tests passed, with the native removal integration test excluded. New regression coverage checks finished-report actions, destination paths, remaining files, and unavailable results. Clippy passed with warnings denied.
- Package verification exercises a simulated completed move and follows both Return and Option-Return through the actual workflow graph to the appropriate native Finder action. These branches terminate without reaching an uninstall handler.
- A temporary Alfred workflow displayed a copy of the saved completed Zen report, with all mutation handlers disabled. Return on the summary opened Finder's Trash; Return on Zen.app selected the actual `~/.Trash/Zen.app`. No files were moved or restored during this check. Option-Return routing was verified by the package checks; physical modifier delivery remains outside the UI driver's reliable capabilities.
- Installed version 0.4.1 with settings preserved and checked every runtime entry against the archive. The previous installation is backed up in `dist/backups/installed-0.4.0-before-result-actions`. The temporary workflow was retired into `dist/result-ui-verification`, outside Alfred's active workflows.

## Finder authorization — 0.4.0

- Replaced the user-level Foundation Trash operation with one native Finder `core/delo` Apple event for the remaining selected files. Interaction is enabled so Finder can display its own Touch ID/password authorization when needed. Paths are encoded as file descriptors, with no AppleScript or shell interpolation.
- Homebrew still runs first for a selected managed app. Already removed entries are skipped; every remaining path is revalidated before the Finder batch.
- Native bookmarks follow Finder moves and name changes. A destination counts as moved only when the original path is absent and the destination matches the scanned device/inode/type. Partial moves remain visible when Finder returns an error or cancellation. Unverified destinations are not counted as successes.
- 40 automated tests passed, covering the existing Homebrew and selection behavior plus batch dispatch, partial cancellation, unverified results, changed-file rejection, exact Unicode/file-path descriptors, bookmark tracking, and distinct Finder error messages.
- The separate native Finder integration test passed outside the development filesystem sandbox: Finder moved one generated text fixture into the real Trash, its bookmark resolved the new location, and the test verified its contents and restored it. No installed application or user data was removed by this test. Authentication for a protected app was not exercised.
- Clippy passed for all targets with warnings denied.
- The packaged workflow passed archive, graph, modifier routing, and packaged-binary checks. Version 0.4.0 was installed with settings preserved; every installed archive entry was compared byte-for-byte. The previous version is backed up in `dist/backups/installed-0.3.0-before-finder-update`.

## File list and keyboard update — 0.3.0

- All discovered entries start selected and display in descending size order, including when app data is larger than the app bundle. The main list contains the uninstall header and native file rows. Scan notices and matching reasons are available separately in Actions → Scan details.
- Return dispatches uninstall directly from the displayed selection. Command-Return dispatches selection changes; Control-Return opens Actions. The graph explicitly connects modifiers to an edit-only Rust handler that rejects removal requests.
- 34 automated tests passed, with the existing native Trash integration test excluded. Tests cover direct execution, default selection, sort order, modifier arguments, modifier-route rejection of uninstall requests, filter preservation, empty selections, separate details, stale actions, and Homebrew behavior.
- Clippy passed with warnings denied. The archive verifier checks the actual modifier connections and exercises the packaged binary's edit handler, including rejection of an uninstall request. It does not execute a removal.
- One fake-Homebrew inventory call timed out during parallel testing; the complete serialized rerun passed (34 passed, 1 ignored).
- The first live shortcut check unexpectedly dispatched uninstall for Zoom. The app move failed with a macOS permission error, and execution stopped before any other file operation. All eight original paths and device/inode identities were checked afterward; no entries were removed or moved. Subsequent keyboard checks use an isolated fixture workflow with all removal disabled.
- The native file-list layout was visually inspected in Alfred with all eight Zoom entries selected and a total of 838.5 MB. The two former boilerplate rows are absent.
- The corrected archive was installed over the existing workflow and every installed archive entry was compared byte-for-byte. The previous installation is backed up in `dist/backups/installed-0.2.0-before-ui-update`.
- The user confirmed that physical Command-Return toggles the file checkbox. The isolated fixture's trace also recorded `edit` with `op: toggle` for the selected file. The UI automation driver had continued dispatching default Return even with explicit modifier connections; Command-A did work.
- After the successful manual check, the temporary `uninstall-ui-check` workflow was removed from Alfred. Its generated app, guarded wrappers, trace, and retired workflow files are retained under `dist/ui-verification` and excluded from the release archive.

## Homebrew support — 0.2.0

- 30 automated tests passed, including fake-Homebrew tests for exact ownership, custom app directories and renamed targets, installed-receipt precedence, cask-before-files ordering, retained registration when keeping the app, noninteractive command flags, failure handling, successful-exit-but-still-installed rejection, hook-removed files, multi-app scope, and bounded command timeouts. The existing native Trash integration test was not repeated because that bridge did not change.
- Clippy passed for all targets with warnings denied.
- The packaged executable passed search → scan → Homebrew confirmation with a fake cask inventory. The verifier rejects any unexpected Homebrew mutation command.
- Read-only scans against the actual installed Homebrew inventory identified `Ghostty.app` as `ghostty` and `Slack.app` as `slack`, without errors. `zoom.us.app` was correctly identified as not managed by an installed cask and still returned eight associated entries.
- No real Homebrew cask was uninstalled. Uninstall behavior is verified with a fake executable and isolated filesystem fixtures. Real cask scripts and administrator requirements can vary; failures stop the workflow's subsequent cleanup and are reported.
- The rebuilt archive is version 0.2.0. Import it into Alfred to update the workflow. This verification did not replace a user's installed workflow or alter their Homebrew inventory.

## Original interface and Trash verification — 0.1.0

- 17 automated tests passed. Tests use temporary app bundles and Library trees; removal tests use an isolated mock Trash directory.
- The separate native integration test passed outside the development filesystem sandbox. Foundation moved a generated text fixture to the real macOS Trash, the test verified its content, and then restored it. It did not move any user app or data.
- Clippy passed for all targets with warnings denied.
- The release archive passed its graph, executable-mode, content-allowlist, and packaged search → scan → review → confirmation checks.
- A read-only scan of `/Applications/zoom.us.app` found all eight entries shown in the supplied Raycast screenshot, totaling approximately 838.5 MB. Bundle-ID matches and the app start selected; the two app-name matches start unchecked.
- A temporary link into Alfred's workflows directory was used to check the actual interface. App search, opening the review, toggling files, filtering paths, preserving the full selection total while filtered, opening confirmation, and the default Back action all worked. The confirmation also correctly described keeping the app installed when its bundle was deselected. No removal action was executed in Alfred.
- Alfred reported restricted access to `~/Library/Cookies`; the review displayed this as an incomplete-scan warning. The terminal's read-only scan had different access. Coverage therefore depends on the permissions macOS grants Alfred.
- The temporary workflow link and its generated review record were removed after testing. The archive is ready to import; the workflow is not left installed by the verification process.

`dist/alfred-review.png` captures the native Alfred review during the selection test. Packaging includes only runtime assets, metadata, documentation, and dependency notices.

Remaining limits: Intel was not tested. Real app removal was not performed. Privileged components and shared group containers are intentionally outside the scanner's scope. Finder/Trash permissions for a particular installed application can still cause a reported move failure.
