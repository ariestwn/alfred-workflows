# Verification — My Schedule 0.1.2

- Rust unit tests: 11 passed; synthetic calendar data only.
- Clippy: all targets, warnings denied, passed.
- Release archive: checksum, file permissions, embedded usage descriptions, native helper code signature, and all 16 Alfred objects verified.
- The installed helper, icons, configuration, and graph match the final package (Alfred may reorder objects and connections).
- The remaining Alfred navigation AppleScript compiled against the installed Alfred dictionary without executing it. Calendar event navigation no longer invokes AppleScript.
- 10,000 synthetic events: title search completed in approximately 16–19 ms on this Mac.
- Live Alfred UI: native calendar events loaded successfully; next meeting, date sections, RSVP labels, attendee counts, and title filtering verified.
- Stable identifiers preserve selection across automatic refreshes.
- Helper search cleanup: build, installed, and backup helper apps carry the `alfred:ignore` Finder tag; existing tags are preserved. Packaging and installation apply it automatically. After reloading Alfred's application cache, live UI search for bare `schedule` showed the agenda with no helper app duplicates. Build/backup helper code signatures remained valid after tagging.
- Calendar navigation: v0.1.2 replaces the Calendar AppleScript path entirely with `ical://ekevent` URLs through Launch Services. Regression tests cover local item IDs, distinct recurring dates, UTC rollover, all-day local dates, identifier escaping, and invalid inputs. Live Enter on a weekly recurring **Next up** meeting opened **14 September 2026, 16:00–17:00** with details and no error. After quitting Calendar normally and confirming no Calendar process was running, Enter on the **21 September 2026** occurrence started Calendar and opened that occurrence with details. Installed and archived v0.1.2 helper binaries match.
- Option-Return's installed modifier payload and conditional destination were inspected: it routes an empty query to the read-only Event Actions Script Filter, which rendered seven expected rows in a fixture run. After explicit user approval, both `alt+Return` and `Alt_L+Return` were attempted through the UI tool. Alfred's debugger received the ordinary `open` payload, so interactive modifier/menu behavior remains unverified; the source of the missing modifier is not established. No menu success is claimed from the fixture test.
- No actual calendar events were created, edited, or deleted, and no conference calls were joined during verification. Create-event previews, rather than writes, were tested.

The sandbox does not resolve application scripting dictionaries correctly. Navigation-script compilation was verified outside the sandbox; a normal Terminal build can access those dictionaries.
