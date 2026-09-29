# My Schedule for Alfred

A Rust workflow for your native macOS Calendar agenda, inspired by Raycast's My Schedule command. It reads Google, iCloud, Exchange, and other calendars already syncing in Calendar via EventKit. A small compiled Objective-C bridge calls Apple's frameworks; agenda logic, filtering, grouping, availability, link detection, and workflow actions run in Rust. No Raycast, Python, Node, or Rust installation is needed at runtime.

## Install

Download the Apple Silicon or Intel build from the [releases page](https://github.com/ariestwn/alfred-workflows/releases) and open it in Alfred. Requires **Alfred 5 with Powerpack** and **macOS 14 or later**. Each build contains a native helper for one architecture only.

Type `schedule`, select **Allow calendar access**, and allow **Full Access** when macOS asks. EventKit requires full access to read your schedule. If access was previously denied, open System Settings → Privacy & Security → Calendars and enable Alfred / My Schedule. Your calendar accounts must already be configured and syncing in the macOS Calendar app.

The first time an action returns to Alfred, macOS may ask for Automation access to Alfred. Opening an event uses Calendar's local URL handler and does not require Calendar Automation access.

## Commands

| Command | Behavior |
| --- | --- |
| `schedule` | Greeting, today's remaining events, next meeting, and upcoming agenda |
| `schedule planning` | Filter event titles; multiple words can appear anywhere in the title |
| `schedule tomorrow` | Tomorrow's events |
| `schedule today design` | Today's events with “design” in the title |
| `schedule week` / `schedule month` | Remaining current week or month |
| `schedule 2026-10-01` | Events on a specific upcoming date |
| `snext` | Next ongoing/upcoming timed event; Return joins if it has a supported call link |
| `scal` | Show/hide calendars; Return toggles, then choose Back to My Schedule |
| `snew` | Create an event with a reviewable preview |
| `sfree today` / `sfree tomorrow` / `sfree week` | Copy free slots within your configured work hours |

Two unassigned Hotkey blocks open the agenda or next meeting directly. Set shortcuts in the workflow editor. Only the main `schedule` keyword is configurable; the helper commands above can be edited on their Script Filter blocks.

## Agenda and event actions

The first row summarizes today; **Next up** shows the next timed event, including an event currently in progress. All-day, canceled, and declined events are excluded from Next up. The summary remains visible while searching so you do not lose track of your next meeting.

Events are grouped into remaining days of this week, next week, the rest of the current month, and subsequent months. This is an Alfred list with title/subtitle rows and section labels, rather than Raycast's custom two-column layout or dropdown. Use `scal` for the calendar picker. Calendar names are accompanied by account/source names so calendars with identical titles can be distinguished.

- **Return:** open the selected event in Calendar, including the chosen occurrence of a recurring event. Calendar starts automatically if it is closed.
- **Command-Return:** join the conference call; disabled if no supported link exists.
- **Option-Return:** open event actions (join, copy conference link, open in Calendar, copy details/title/attendees).
- **Shift-Return:** copy event details.
- **Command-Shift-Return:** copy event title.
- **Control-Return:** copy attendees.
- **Command-C:** copy event details. **Command-L:** show details in Large Type.
- **Option-Return on the greeting:** choose calendars.

Event rows show date/time, calendar, RSVP status when available, attendee count, recurrence, and conference provider. Declined invitations are hidden by default; all-day events are shown. Both are configurable. Canceled events are always hidden. Live results refresh every five seconds, using an EventKit snapshot cached for at most twenty seconds. **Refresh schedule** forces a fresh read. Large agendas show 250 matching events at a time; use title or date filters to find later events. Look ahead defaults to 90 days and supports up to 365.

Version 0.1.2 replaces Calendar AppleScript navigation after both `AppleEvent handler failed (-10000)` and `Connection is invalid (-609)` were reported. It opens Calendar directly with the local EventKit item identifier and occurrence date. Timed recurring events use UTC; all-day recurring events use the Mac's local date, independently of the agenda display timezone. Non-recurring events use their item identifier alone. The Calendar URL convention is also used in Raycast's [Menu Bar Calendar extension](https://github.com/raycast/extensions/blob/36abdaacac9b02cbbc54dbe33f16b6c40cd23f54/extensions/menubar-calendar/swift/AppleReminders/Sources/Calendar.swift).

## Conference calls

Links are detected in the event URL, location, and notes. Supported host names cover Zoom, Google Meet, Microsoft Teams, Slack Huddles, Webex, FaceTime, Skype, BlueJeans, Amazon Chime, Whereby, Jitsi (`meet.jit.si` / `jitsi.org`), Around, Chorus, Riverside, and StreamYard. Self-hosted Jitsi domains and indirect redirect links are not auto-detected. Provider links use HTTPS and are checked against domain boundaries before opening.

Zoom `/j/` links open the Zoom app when installed, and Microsoft Teams links open the current Teams app when installed. Other links use the browser or the provider's usual app handoff. Set a preferred Google Meet browser in Workflow Configuration, or leave the default browser selected. Links are opened only when you choose Join; joining does not enable the microphone or camera itself.

## Create an event / block focus time

Type `snew Focus time | tomorrow 14:00 | 60`. A preview appears for each writable calendar, with the default calendar first. Press Return on the desired calendar to save. Typing and browsing previews never creates events.

The full input is:

```text
snew Title | start | minutes | calendar | location | HTTPS URL | notes
```

Only the first three fields are required. Leave optional fields blank to skip them. Calendar matching uses an exact title or identifier; matching calendars from different accounts remain separate choices. Times accept `today 14:00`, `tomorrow 14:00`, `YYYY-MM-DD HH:MM`, or an ISO timestamp with an explicit offset. The display timezone defaults to macOS and can be overridden. Invalid or repeated DST clock times require an explicit ISO offset. Durations are 1 minute to 7 days. Saved events open in Calendar for further editing. Identical submitted previews are recorded locally to prevent accidental duplicate creation.

EventKit does not expose setters for attendee RSVP status or attendees. Use **Open in Calendar** to accept/decline invitations, manage attendees, add reminders, change recurrence, edit, or delete events. Creating events through this workflow does not invite or email anyone.

## Availability

`sfree` lists free intervals across your **enabled calendars**. Work hours default to 09:00–17:00, with a minimum free slot of 30 minutes. `week` means the next seven calendar days, excluding Saturday/Sunday; an explicitly selected date can be a weekend. Overlapping busy events are merged. All-day busy events still block the day even when hidden from the agenda. Events marked Free, canceled events, and declined invitations do not block time. Tentative and pending invitations count as busy. Today's slots start no earlier than now. Holidays only block time if represented by a busy event on an enabled calendar.

## Scope and privacy

This implements the My Schedule agenda, title search, calendar selection, manual call joining, copying details/availability, and event creation with a preview. Raycast-specific AI chat, arbitrary action-panel shortcuts, auto-join, camera preview, transcription, and a persistent menu-bar app are not included. Invitation changes, attendee management, advanced editing, and deletion use the native Calendar app.

There are no external services, API keys, analytics, or background daemons. Calendar data stays on this Mac; its accounts continue syncing normally through macOS. Private snapshots live in Alfred's workflow cache with mode 0600, and visibility preferences/submitted-create markers live in Alfred's workflow data directory. The export contains no event data or account identifiers. Uninstalling the workflow does not delete any calendar events.

## Build and verify

```sh
./scripts/cargo.sh test --locked --offline
./scripts/cargo.sh clippy --locked --offline --all-targets -- -D warnings
./build.sh
```

Requires Rust, Xcode command line tools, and Python 3 at build time. The wrapper uses Cargo from PATH or the sibling `alfred-quickai/.tools` toolchain. Dependencies must be cached first (`./scripts/cargo.sh fetch --locked` if needed). The build compiles the EventKit bridge, embeds Calendar usage descriptions, signs the helper app, renders icons, bundles dependency notices, and validates the workflow archive.

Packaging and the local installer tag build, installed, and backup helper apps with `alfred:ignore`, preserving existing Finder tags. This keeps technical copies out of Alfred's application results while `schedule` continues to run the workflow. The convention is documented by the [Alfred team](https://alfred.app/workflows/alfredapp/ignore-in-alfred/).

Tests use synthetic events, deterministic dates, and temporary directories; they do not read real calendars, join calls, or create events. `--fixture /path/to/snapshot.json` is available on read-only CLI commands for testing. `status` reads only the macOS authorization state. EventKit write operations are never used by automated tests.

References: [Apple EventKit access](https://developer.apple.com/documentation/eventkit/accessing-the-event-store), [Alfred Script Filter JSON](https://www.alfredapp.com/help/workflows/inputs/script-filter/json/). Calendar's installed URL handler supports the `ical://ekevent` navigation scheme.
