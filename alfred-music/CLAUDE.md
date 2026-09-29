# Apple Music + Audio Format

macOS menubar app + Alfred workflow for inspecting and switching the audio output format on the default device. Combines:

- **Menubar app** — SwiftUI now-playing card (artwork, track, format, transport controls, format switcher) inside an `NSPanel` floated below an `NSStatusItem`.
- **Background daemon** — same binary; tails the unified system log for Music.app's MediaToolbox format-report line and mirrors the live audio format into a JSON cache.
- **Alfred workflow** — keyword-driven access to the same data + actions.

The daemon and the menubar UI are one process registered as a user `LaunchAgent` (`com.ariestwn.apple-music-audio-format`). KeepAlive respawns it on crash.

## Project layout

```
applemusic-nowplaying/
├── build.sh                  Single entry point — compile, assemble, zip
├── src/                      Swift sources + .app Info.plist template
│   ├── app.swift             Menubar UI + log watcher + iTunes artwork fetch
│   ├── audio_format.swift    CLI helper for the Alfred `audio` keyword
│   └── AppBundle.Info.plist  .app bundle Info template (LSUIElement, NSAppleEventsUsageDescription)
├── workflow/                 Flat layout Alfred sees (zipped into .alfredworkflow)
│   ├── info.plist            Alfred workflow definition
│   ├── icon.png              Workflow icon (256×256, also source for .app's AppIcon.icns)
│   ├── show.sh               Alfred Script Filter for `np`
│   ├── autoapply{,_set}.sh   Toggle UI for `npauto`
│   ├── nplog{,_action}.sh    Log viewer for `nplog`
│   ├── menubar{,_set}.sh     Show / hide menubar icon for `npmenu`
│   ├── install.sh            Compile + install LaunchAgent + thin universal binary to host arch
│   ├── uninstall.sh          Bootout + remove plist + delete support folder
│   └── (built — gitignored)
│       ├── audio_format                       Universal Mach-O for `audio` keyword + auto-apply path
│       └── Apple Music Audio Watcher.app/     Daemon bundle assembled by build.sh
└── dist/                     Gitignored
    └── Apple Music + Audio Format.alfredworkflow   Zipped contents of workflow/
```

## Architecture

### Two binaries, one app

- **`Apple Music Audio Watcher`** — menubar/daemon binary inside `Apple Music Audio Watcher.app/Contents/MacOS/`. Compiled from `src/app.swift`.
- **`audio_format`** — small CLI compiled from `src/audio_format.swift`. Lives next to the workflow's Alfred scripts; called by the `audio` keyword Script Filter and the watcher's auto-apply path.

Both are universal binaries (arm64 + x86_64). `workflow/install.sh` thins them to the host arch on install — distribution stays cross-platform, installed footprint is half-size native.

### Daemon flow (`src/app.swift`)

```
┌────────────────────────┐         ┌──────────────────────────┐
│ /usr/bin/log stream    │ ──────► │ LogWatcher.handle        │
│ filtered to Music.app  │         │  parses [SampleRate ...] │
│ MediaToolbox           │         │  writes nowplaying.json  │
└────────────────────────┘         │  triggers auto-apply     │
                                    └────────┬─────────────────┘
                                             │
                                             ▼
                                    ┌──────────────────────────┐
                                    │ PlayerStore.refresh      │
                                    │  AppleScript meta query  │
                                    │  detects track-key change│
                                    │  fires fetchArtwork()    │
                                    └────────┬─────────────────┘
                                             │
                                             ▼
                                    ┌──────────────────────────┐
                                    │ NowPlayingCard (SwiftUI) │
                                    │  inside NSPanel popover  │
                                    └──────────────────────────┘
```

### Cache files (`~/Library/Caches/com.ariestwn.apple-music-audio-format/`)

| File | Producer | Consumer |
|---|---|---|
| `nowplaying.json` | LogWatcher (per track-start) | Alfred `np` script + PlayerStore |
| `apply.log` | LogWatcher (per successful auto-apply) | Alfred `nplog` script |
| `artwork.jpg` | PlayerStore (200×200 from iTunes Search API) | NowPlayingCard SwiftUI |
| `watcher.err` / `watcher.out` | LaunchAgent stdio | manual debugging |

### Toggle flag files (`~/Library/Application Support/com.ariestwn.apple-music-audio-format/`)

| File | Effect |
|---|---|
| `autoapply.off` | Daemon stops calling CoreAudio set on track change |
| `menubar.off` | NSStatusItem hidden (daemon keeps running) |

Both are watched via `DispatchSource` on the support folder — toggles reflect immediately, no daemon restart needed.

## Alfred keywords

| Keyword | Script (in workflow/) | Action |
|---|---|---|
| `audio` | `audio_format list` | Switch default output sample rate / bit depth |
| `np` | `show.sh` | Show current track + format (Enter copies to clipboard) |
| `npauto` | `autoapply.sh` / `autoapply_set.sh` | Toggle auto-follow |
| `nplog` | `nplog.sh` / `nplog_action.sh` | View / clear apply log |
| `npmenu` | `menubar.sh` / `menubar_set.sh` | Show / hide menubar icon |

## Build & install

```bash
cd applemusic-nowplaying
./build.sh                    # compile src/ → workflow/, then zip workflow/ → dist/
./workflow/install.sh         # install LaunchAgent + thin to host arch + bootstrap
./workflow/uninstall.sh       # bootout + remove plist + delete support folder
```

For Alfred workflow distribution: hand the user `dist/Apple Music + Audio Format.alfredworkflow`. Double-click to import — Alfred extracts `workflow/` flat into its workflow folder. The workflow's own `install.sh` runs on first invocation to set up the LaunchAgent.

`workflow/install.sh` is idempotent (bootout-then-bootstrap with retries for launchd reentrancy).

## Permissions

The daemon needs three macOS permissions that are NOT auto-prompted from a launchctl-spawned background app:

1. **Automation → Music** — set on first AppleScript call to Music.app (auto-prompt works).
2. **Automation → System Events** — same (auto-prompt works).
3. **Accessibility** — required for shuffle/repeat (UI scripting via System Events). Auto-prompt is triggered programmatically via `AXIsProcessTrustedWithOptions` once per session.

If shuffle/repeat silently fail with `error -1719 (assistive access)`:

```bash
tccutil reset Accessibility com.ariestwn.apple-music-audio-format
```

then click any control button — the prompt re-appears with the correct (current) bundle path. macOS sometimes caches stale path/signature attribution after rebuilds.

## Key technical details

- **macOS 13+** required (Swift literal regex, `kAudioObjectPropertyElementMain`).
- **Ad-hoc codesigned** (`codesign --sign -`) so the .app shows up in System Settings → Login Items & Extensions with a stable identifier. Without an Apple Developer ID, the entry shows "Item from unidentified developer" with a generic icon — a cosmetic-only limit; the daemon functions normally.
- **Artwork** is fetched from the iTunes Search API (public, unauthenticated) per track change, downloaded as a 200×200 JPEG to `artwork.jpg`. Tried first: `artworks of current track` AppleScript (returns 0 for streaming), MediaRemote `MRMediaRemoteGetNowPlayingInfo` (Apple removed third-party access in macOS 14+), MusicKit catalog search (requires paid Developer ID entitlement). iTunes Search API is the surviving public path.
- **Format auto-apply** uses the same CoreAudio HAL calls as the `audio_format` CLI (`kAudioStreamPropertyPhysicalFormat`). Bit-depth fallback chain (exact → 24 → 32 → 16) handles DACs that don't expose every bit-depth at every rate.
- **NSStatusItem** click routing: left-click → SwiftUI now-playing popover (`NSPanel` with `.menu` material vibrancy). Right-click → action menu (Switch Format, auto-apply toggle, hide icon, Open in Alfred, Quit).
- **`hidesOnDeactivate = false`** on the panel — accessory + nonactivating app deactivates immediately when the panel shows; without this the panel orderFronts then yanks back off-screen one frame later.
- **`NowPlayingPanel.canBecomeKey = true`** so the SwiftUI Button transport controls receive clicks; `becomesKeyOnlyIfNeeded = true` keeps Music.app's keyboard focus on the menubar app's behalf.
- **Shuffle / Repeat** can't be toggled via Music.app's `shuffle enabled` / `song repeat` properties (silently no-op in recent versions). Use System Events UI scripting to drive the `Controls → Shuffle / Repeat` menu items, run via in-process `NSAppleScript` so Accessibility attribution lands on our app's bundle (not `/usr/bin/osascript`, which the user can't grant — it's on the read-only system volume).

## Common debugging

| Symptom | Check |
|---|---|
| Menubar icon not showing | Check `Ice` / Bartender / similar — they hide unsigned items by default. Restart macOS or re-grant Accessibility if cached attribution is stale. |
| Click does nothing | `pgrep -fl "Apple Music Audio Watcher"` — orphan from previous run? Kill, let KeepAlive respawn. |
| Shuffle / repeat fail silently | `tccutil reset Accessibility com.ariestwn.apple-music-audio-format` then click a control — re-prompts. |
| Wrong artwork | iTunes Search API matches by title+artist; very generic titles can collide. Track change refreshes on next play. |
| `Bootstrap failed: 5: Input/output error` | launchd reentrancy after rapid bootout/bootstrap. `bootout` + 6-second wait + `bootstrap`. |
| Alfred `np` returns "AAC" for a library track | The track is a "shared track" (Apple Music streaming, not in your library) OR you have Lossless disabled. Daemon's parsing is correct. |
