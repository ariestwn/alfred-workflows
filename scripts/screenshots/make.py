#!/usr/bin/env python3
"""Render the README screenshots from each workflow's real output, using demo data only.

Every workflow binary runs against temporary fixtures, never your calendar,
screenshots, apps, or chats. Results are drawn in an Alfred-style window and
captured with headless Chromium (Playwright's chrome-headless-shell if
installed, otherwise Google Chrome) into docs/screenshots/.

Run ./scripts/release.sh (or each project's build.sh) first. The QuickAI and
Upload to Remote status popups are rendered by their debug builds
(./scripts/cargo.sh build) and briefly appear on screen while this runs.
"""
import datetime
import html
import json
import os
import plistlib
import re
import shutil
import subprocess
import tempfile
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs/screenshots"
HEADLESS_SHELL = sorted(Path.home().glob(
    "Library/Caches/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-mac-*/chrome-headless-shell"))
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
TMP = Path(tempfile.mkdtemp(prefix="alfred-shots-"))
WIDTH = 960

CSS = """
* { box-sizing: border-box; margin: 0; padding: 0; }
html, body { width: %(w)dpx; height: %(h)dpx; overflow: hidden; }
body { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 26px;
  background: radial-gradient(130%% 85%% at 50%% 118%%, #3b4cf0 0%%, #1d2396 27%%, #08081c 55%%, #000 78%%);
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif; -webkit-font-smoothing: antialiased; color: #f2f2f4; }
.alfred { width: 720px; background: rgba(23, 21, 27, .97); border: 1px solid rgba(255,255,255,.17);
  border-radius: 18px; box-shadow: 0 28px 80px rgba(0,0,0,.6); padding: 8px 10px 10px; }
.query { font-size: 34px; font-weight: 300; height: 60px; line-height: 60px; padding: 0 14px; white-space: nowrap; overflow: hidden; }
.caret { display: inline-block; width: 2px; height: 36px; background: #eee; vertical-align: -7px; margin-left: 1px; }
.head { display: flex; align-items: center; gap: 12px; padding: 8px 14px 4px; }
.head img { width: 44px; height: 44px; }
.row { display: flex; align-items: center; gap: 12px; height: 52px; padding: 0 12px; border-radius: 10px; }
.row.sel { background: rgba(255,255,255,.09); }
.row img { width: 32px; height: 32px; object-fit: contain; flex: none; }
.text { flex: 1; min-width: 0; }
.title { font-size: 17px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.sub { font-size: 12.5px; color: #a3a3a9; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 1px; }
.key { font-size: 14px; color: #c6c6cb; width: 32px; text-align: right; flex: none; }
.grid { display: grid; gap: 6px; padding: 2px 4px 6px; }
.tile { border-radius: 12px; padding: 10px 8px 8px; text-align: center; }
.tile.sel { background: rgba(255,255,255,.1); }
.tile img { width: 100%%; aspect-ratio: 1; object-fit: contain; display: block; }
.label { font-size: 12px; color: #d2d2d7; margin-top: 6px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.textview { height: 400px; overflow: hidden; padding: 4px 18px 10px; font-size: 14.5px; line-height: 1.55; color: #e8e8ec; }
.textview h1, .textview h2 { font-size: 17px; font-weight: 600; margin: 12px 0 4px; }
.textview p, .textview ul, .textview pre { margin: 6px 0; }
.textview ul { padding-left: 20px; }
.textview code { font-family: ui-monospace, "SF Mono", Menlo, monospace; font-size: 13px; background: rgba(255,255,255,.08); padding: 1px 5px; border-radius: 5px; }
.textview pre { background: rgba(255,255,255,.06); border-radius: 8px; padding: 10px 12px; }
.textview pre code { background: none; padding: 0; }
.footer { border-top: 1px solid rgba(255,255,255,.1); padding: 8px 14px 2px; font-size: 12px; color: #9d9da3; }
.hud { display: block; }
"""


def esc(value):
    return html.escape(str(value))


def src(icon, base):
    """Resolve an Alfred item icon to a file URI."""
    icon = icon or {}
    if icon.get("type") == "fileicon":
        return FILE_ICONS[icon["path"]].as_uri()
    return (base / icon.get("path", "icon.png")).resolve().as_uri()


def capture(doc, png, width, height, scale):
    page_path = TMP / (png.stem + ".html")
    page_path.write_text(doc)
    png.unlink(missing_ok=True)
    browser = [str(HEADLESS_SHELL[-1])] if HEADLESS_SHELL else [CHROME, "--headless=new", "--no-first-run", "--use-mock-keychain"]
    process = subprocess.Popen(browser + [
        "--disable-gpu", "--hide-scrollbars", "--default-background-color=00000000", "--force-device-scale-factor=%d" % scale,
        "--user-data-dir=" + str(TMP / "chrome"), "--window-size=%d,%d" % (width, height),
        "--screenshot=" + str(png), page_path.as_uri()], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    # Full Chrome can keep running after it writes the screenshot.
    deadline = time.monotonic() + 60
    while process.poll() is None and not png.exists() and time.monotonic() < deadline:
        time.sleep(0.1)
    if process.poll() is None:
        time.sleep(1)
        process.kill()
        process.wait()
    assert png.exists(), "Browser did not write " + str(png)


def page(body, height, out):
    doc = "<!doctype html><meta charset=utf-8><style>%s</style><body>%s</body>" % (CSS % dict(w=WIDTH, h=height), body)
    OUT.mkdir(parents=True, exist_ok=True)
    capture(doc, OUT / (out + ".png"), WIDTH, height, 2)
    print("wrote", (OUT / (out + ".png")).relative_to(ROOT))


def query(text):
    return '<div class="query">%s<span class="caret"></span></div>' % esc(text)


def rows(items, base):
    parts = []
    for i, item in enumerate(items):
        key = "↩" if i == 0 else "⌘%d" % (i + 1)
        parts.append('<div class="row%s"><img src="%s"><div class="text"><div class="title">%s</div>'
                     '<div class="sub">%s</div></div><div class="key">%s</div></div>'
                     % (" sel" if i == 0 else "", src(item.get("icon"), base), esc(item["title"]),
                        esc(item.get("subtitle", "")), key))
    return "".join(parts)


def hud_img(png):
    width = int(subprocess.check_output(["sips", "-g", "pixelWidth", str(png)], text=True).split()[-1]) // 2
    height = int(subprocess.check_output(["sips", "-g", "pixelHeight", str(png)], text=True).split()[-1]) // 2
    return '<img class="hud" src="%s" width="%d" height="%d">' % (png.as_uri(), width, height), height


def list_shot(out, typed, items, base, hud=None, head=""):
    body = '<div class="alfred">%s%s%s</div>' % (head, query(typed), rows(items, base))
    height = 8 + 60 + 52 * len(items) + 12 + (66 if head else 0)
    if hud:
        tag, hud_height = hud_img(hud)
        body += tag
        height += 26 + hud_height
    page(body, height + 120, out)


def grid_shot(out, typed, items, base, columns):
    tile = (690 - 6 * (columns - 1)) // columns
    cells = "".join('<div class="tile%s"><img src="%s"><div class="label">%s</div></div>'
                    % (" sel" if i == 0 else "", src(item.get("icon"), base), esc(item["title"]))
                    for i, item in enumerate(items))
    body = ('<div class="alfred">%s<div class="grid" style="grid-template-columns: repeat(%d, minmax(0, 1fr))">'
            '%s</div></div>' % (query(typed), columns, cells))
    lines = -(-len(items) // columns)
    page(body, 90 + lines * (tile + 29) + 120, out)


def markdown(text):
    """Enough Markdown for a chat transcript: headings, lists, code, bold."""
    blocks, parts = re.split(r"```\w*\n(.*?)```", text, flags=re.S), []
    for i, block in enumerate(blocks):
        if i % 2:
            parts.append("<pre><code>%s</code></pre>" % esc(block.rstrip()))
            continue
        for chunk in re.split(r"\n\s*\n", block.strip()):
            if not chunk:
                continue
            inline = re.sub(r"`([^`]+)`", r"<code>\1</code>", esc(chunk))
            inline = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", inline)
            if chunk.startswith("#"):
                parts.append("<h2>%s</h2>" % inline.lstrip("# "))
            elif all(line.startswith("- ") for line in chunk.splitlines()):
                parts.append("<ul>%s</ul>" % "".join("<li>%s</li>" % l[2:] for l in inline.splitlines()))
            else:
                parts.append("<p>%s</p>" % inline.replace("\n", "<br>"))
    return "".join(parts)


def text_shot(out, typed, response, footer):
    body = '<div class="alfred">%s<div class="textview">%s</div><div class="footer">%s</div></div>' % (
        query(typed), markdown(response), esc(footer))
    page(body, 8 + 60 + 400 + 34 + 12 + 120, out)


def run(args, cwd, **env):
    scratch = TMP / cwd.parent.name
    environment = dict(os.environ, alfred_workflow_cache=str(scratch / "cache"),
                       alfred_workflow_data=str(scratch / "data"), **env)
    return json.loads(subprocess.check_output(args, cwd=cwd, env=environment, text=True))


def keyword(base, word):
    info = plistlib.loads((base / "info.plist").read_bytes())
    config = next(o["config"] for o in info["objects"]
                  if o["type"].endswith("input.keyword") and o["config"]["keyword"] == word)
    return dict(title=config["text"], subtitle=config["subtext"])


def render_hud(binary, prefix, snapshot_variable, working, done):
    """Show the workflow's real HUD and keep its debug build's view-only PNG.

    working and done are (title, detail) pairs, matching the texts in the workflow's src/hud.rs.
    """
    snapshots = TMP / (prefix + "png")
    snapshots.mkdir()
    directory = Path(tempfile.mkdtemp(prefix=prefix, dir=os.environ.get("TMPDIR", tempfile.gettempdir())))
    state = dict(phase="working", title=working[0], detail=working[1],
                 owner=os.getpid(), expires=int(time.time() * 1000) + 15000)

    def save():
        (directory / "next.json").write_text(json.dumps(state))
        (directory / "next.json").replace(directory / "state.json")

    save()
    process = subprocess.Popen([str(binary), "--hud", str(directory)],
                               env=dict(os.environ, **{snapshot_variable: str(snapshots)}))
    deadline = time.monotonic() + 5
    while not (directory / "ready").exists():
        assert time.monotonic() < deadline and process.poll() is None, "HUD did not start"
        time.sleep(0.05)
    time.sleep(0.4)
    state.update(phase="success", title=done[0], detail=done[1], owner=0)
    save()
    process.wait(timeout=15)
    return snapshots / "success.png"


def file_icons(paths):
    jobs = [dict(path=str(p), out=str(TMP / ("icon-%d.png" % i))) for i, p in enumerate(paths)]
    subprocess.run(["/usr/bin/swift", str(Path(__file__).with_name("fileicon.swift"))], input=json.dumps(jobs),
                   text=True, check=True, env=dict(os.environ, CLANG_MODULE_CACHE_PATH=str(TMP / "clang")))
    FILE_ICONS.update({job["path"]: Path(job["out"]) for job in jobs})


FILE_ICONS = {}

# Twelve made-up "screenshots" for the Screenshots workflow's demo folder.
MOCKUPS = [("Sprint board", "#6366f1"), ("Sales dashboard", "#0ea5e9"), ("Design review", "#ec4899"),
           ("Pull request #482", "#22c55e"), ("Team chat", "#f59e0b"), ("Travel map", "#14b8a6"),
           ("Invoice INV-2041", "#8b5cf6"), ("Terminal", "#64748b"), ("Photo edit", "#ef4444"),
           ("Release notes", "#84cc16"), ("Analytics", "#06b6d4"), ("Settings", "#a855f7")]


def mockup(title, accent, out):
    cards = "".join('<div style="background:#fff;border-radius:14px;height:%dpx;box-shadow:0 2px 8px rgba(0,0,0,.08);'
                    'border-top:6px solid %s"></div>' % (120 + (i * 37) % 90, accent) for i in range(6))
    doc = ('<!doctype html><body style="margin:0;width:1280px;height:800px;'
           'font-family:system-ui,-apple-system,BlinkMacSystemFont,Helvetica Neue,sans-serif;'
           'background:linear-gradient(135deg,%s,#0f172a)"><div style="position:absolute;inset:60px 90px;background:#f1f5f9;'
           'border-radius:16px;overflow:hidden;box-shadow:0 30px 60px rgba(0,0,0,.35)"><div style="height:44px;background:#e2e8f0;'
           'display:flex;align-items:center;gap:8px;padding:0 16px"><i style="width:12px;height:12px;border-radius:6px;background:#f87171"></i>'
           '<i style="width:12px;height:12px;border-radius:6px;background:#fbbf24"></i><i style="width:12px;height:12px;border-radius:6px;'
           'background:#34d399"></i></div><div style="display:flex;height:100%%"><div style="width:220px;background:#e2e8f0"></div>'
           '<div style="flex:1;padding:28px 34px"><div style="font-size:40px;font-weight:700;color:#0f172a;margin-bottom:22px">%s</div>'
           '<div style="display:grid;grid-template-columns:repeat(3,1fr);gap:18px">%s</div></div></div></div></body>'
           % (accent, esc(title), cards))
    capture(doc, out, 1280, 800, 1)


def calculator():
    base = ROOT / "alfred-calculator/workflow"
    items = run(["./calculator", "filter", "--", ""], base, CALC_DECIMAL="dot")["items"]
    list_shot("calculator", "calc ", items[:7], base)


def emoji():
    base = ROOT / "alfred-emoji/workflow"
    items = [i for i in run(["./emoji", "happy"], base)["items"] if i.get("variables", {}).get("EMOJI_KIND") == "emoji"]
    grid_shot("emoji", "emoji happy", items[:12], base, 6)


def handy():
    base = ROOT / "alfred-handy/workflow"
    items = run(["./handy", "filter", "--", ""], base, HANDY_DATA_DIR=str(TMP / "handy"))["items"]
    list_shot("handy", "handy ", items[:7], base)


def upload():
    base = ROOT / "alfred-kraely-upload/workflow"
    desktop = TMP / "Desktop"
    desktop.mkdir()
    shutil.copy(TMP / "shots" / sorted(os.listdir(TMP / "shots"))[0], desktop / "design-review.png")
    file_icons([desktop / "design-review.png"])
    head = ('<div class="head"><img src="%s"><div class="text"><div class="title">design-review.png</div>'
            '<div class="sub">~/Desktop</div></div></div>' % FILE_ICONS[str(desktop / "design-review.png")].as_uri())
    hud = render_hud(ROOT / "alfred-kraely-upload/target/debug/alfred-kraely-upload", "kraely-hud-",
                     "KRAELY_HUD_SNAPSHOT_DIR", ("Uploading to remote…", "Upload to Remote"),
                     ("Upload to Remote · Done", "Remote path copied. Paste with ⌘V."))
    action = dict(title="Upload to Remote", subtitle="Upload the file over SSH and copy its absolute remote path")
    list_shot("upload-to-remote", "upload", [action], base, hud=hud, head=head)


def schedule():
    base = ROOT / "alfred-my-schedule/workflow"
    now = datetime.datetime.now().astimezone()
    today = now.replace(minute=0, second=0, microsecond=0) + datetime.timedelta(hours=1)
    tomorrow = (now + datetime.timedelta(days=1)).replace(hour=9, minute=30, second=0, microsecond=0)
    people = [dict(name=n, email="mailto:%s@example.com" % n.lower(), status=2) for n in ["Ana", "Sam", "Lee", "Kai"]]

    def event(ident, title, start, minutes, calendar="work", **extra):
        return dict(id=ident, calendar_id=calendar, title=title, start=int(start.timestamp()),
                    end=int((start + datetime.timedelta(minutes=minutes)).timestamp()), self_status=2, **extra)

    events = [
        event("1", "Design review", today, 45, url="https://meet.google.com/abc-defg-hij", attendees=people),
        event("2", "1:1 with Sam", today + datetime.timedelta(hours=2), 30, url="https://zoom.us/j/123456789",
              attendees=[dict(name="You", status=2, is_self=True), people[1]]),
        event("3", "Team standup", tomorrow, 15, recurring=True, url="https://meet.google.com/xyz-abcd-efg",
              attendees=people),
        event("4", "Lunch with Ana", tomorrow.replace(hour=12, minute=30), 60, calendar="home"),
        event("5", "Focus time", tomorrow.replace(hour=14, minute=0), 120),
        event("6", "Quarterly planning", tomorrow.replace(hour=10) + datetime.timedelta(days=2), 90,
              url="https://teams.microsoft.com/l/meetup-join/demo", attendees=people),
    ]
    events[5]["self_status"] = 4  # Tentative
    fixture = TMP / "schedule.json"
    fixture.write_text(json.dumps(dict(
        calendars=[dict(id="work", title="Work", source="Google", writable=True),
                   dict(id="home", title="Personal", source="iCloud", writable=True)],
        events=events, default_calendar="work")))
    items = run(['./My Schedule.app/Contents/MacOS/my-schedule', "agenda", "--fixture", str(fixture), "--", ""], base)["items"]
    list_shot("my-schedule", "schedule ", items[:7], base)


def quickai():
    base = ROOT / "alfred-quickai/workflow"
    hud = render_hud(ROOT / "alfred-quickai/target/debug/alfred-quickai", "quickai-hud-", "QUICKAI_HUD_SNAPSHOT_DIR",
                     ("Checking grammar…", "QuickAI"), ("QuickAI · Done", "Paste sent to your app."))
    list_shot("quickai", "grammar-check She dont like apples.", [keyword(base, "grammar-check")], base, hud=hud)


def screenshots():
    base = ROOT / "alfred-screenshots-rust/workflow"
    folder = TMP / "shots"
    items = run(["./shots", "browse", ""], base, SHOTS_FOLDER=str(folder), SHOTS_PAGE_SIZE="12")["items"]
    items = [i for i in items if i.get("variables", {}).get("SHOTS_KIND") != "page"]
    grid_shot("screenshots", "shots", items[:8], base, 4)


def uninstaller():
    base = ROOT / "alfred-uninstaller/workflow"
    home = TMP / "home"
    app = home / "Applications/Sketchpad.app"
    (app / "Contents/MacOS").mkdir(parents=True)
    (app / "Contents/Resources").mkdir()
    (app / "Contents/Info.plist").write_bytes(plistlib.dumps(dict(
        CFBundleIdentifier="com.example.sketchpad", CFBundleName="Sketchpad", CFBundleExecutable="sketchpad",
        CFBundlePackageType="APPL", CFBundleIconFile="AppIcon")))
    # Finder badges apps without a real executable and icon as unable to open.
    capture('<body style="margin:0;background:transparent"><div style="margin:50px;width:412px;height:412px;'
            'border-radius:92px;background:linear-gradient(160deg,#fb923c,#db2777);display:flex;align-items:center;'
            'justify-content:center;font-size:250px">🎨</div>', TMP / "sketchpad.png", 512, 512, 1)
    subprocess.run(["sips", "-s", "format", "icns", str(TMP / "sketchpad.png"), "--out",
                    str(app / "Contents/Resources/AppIcon.icns")], check=True, capture_output=True)
    library = home / "Library"
    sizes = {app / "Contents/MacOS/sketchpad": 148_000_000,
             library / "Caches/com.example.sketchpad/cache.db": 212_000_000,
             library / "Application Support/com.example.sketchpad/projects.db": 64_500_000,
             library / "Logs/Sketchpad/sketchpad.log": 2_300_000,
             library / "HTTPStorages/com.example.sketchpad/httpstorages.sqlite": 180_000,
             library / "Saved Application State/com.example.sketchpad.savedState/data.data": 42_000,
             library / "Preferences/com.example.sketchpad.plist": 4_096}
    shutil.copy("/usr/bin/true", app / "Contents/MacOS/sketchpad")
    for path, size in sizes.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.touch()
        os.truncate(path, size)
    brew = TMP / "brew"
    brew.write_text('#!/bin/sh\nif [ "$1" = info ]; then echo \'{"casks":[]}\'; else pwd -P; fi\n')
    brew.chmod(0o755)
    env = dict(HOME=str(home), UNINSTALL_BREW_PATH=str(brew))
    listing = run(["./uninstaller", "list", "Sketchpad"], base, **env)
    item = next(i for i in listing["items"] if i.get("valid"))
    variables = run(["./uninstaller", "action", item["arg"]], base, **env)["alfredworkflow"]["variables"]
    items = run(["./uninstaller", "view", ""], base, **env, **variables)["items"]
    file_icons([i["icon"]["path"] for i in items if i.get("icon", {}).get("type") == "fileicon"])
    list_shot("uninstaller", "", items[:7], base)


def ai_chat():
    base = ROOT / "alfred-ai-chat/workflow"
    data = TMP / "alfred-ai-chat" / "data"
    (data / "codex").mkdir(parents=True)
    answer = ("A **process** is a running program with its own isolated memory, file handles, and security context. "
              "A **thread** is a unit of execution inside a process; threads share the process's memory, so they are "
              "cheaper to create but need locks to share data safely.\n\nOn macOS, list the threads of a process with:\n\n"
              "```sh\nps -M -p <pid>\n```\n\nFor a live view, open Activity Monitor, double-click the process, and "
              "choose **Sample**.")
    chat = dict(id=str(uuid.uuid4()), provider="codex", updated=time.time(), messages=[
        dict(role="user", content="What is the difference between a process and a thread? How do I list the "
                                  "threads of a process on macOS?", status="complete"),
        dict(role="assistant", content=answer, status="complete")])
    (data / "codex/chat.json").write_text(json.dumps(chat))
    result = run(["./ai-chat", "view", ""], base, AI_PROVIDER="codex")
    text_shot("ai-chat", "How do I sample it from Terminal?", result["response"], result["footer"])


def main():
    (TMP / "shots").mkdir()
    start = time.time() - 3600
    for i, (title, accent) in enumerate(MOCKUPS):
        path = TMP / "shots" / ("Screenshot 2026-09-%02d at %02d.%02d.00.png" % (28 - i // 4, 9 + i, (i * 7) % 60))
        mockup(title, accent, path)
        os.utime(path, (start - i * 5400, start - i * 5400))
    for shot in [calculator, emoji, handy, schedule, screenshots, uninstaller, ai_chat, quickai, upload]:
        shot()
    shutil.rmtree(TMP, ignore_errors=True)


if __name__ == "__main__":
    main()
