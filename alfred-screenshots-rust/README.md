# Screenshots Rust

A separate Alfred screenshot browser built in Rust, for folders containing thousands of screenshots. Inspired by the browsing controls in the installed Screenshots workflow. The original path supplied for this task was ChatGPT / DALL-E; the confirmed scope here is **browsing and selecting screenshots**.

## Install

Download the Apple Silicon or Intel build from the [releases page](https://github.com/ariestwn/alfred-workflows/releases) and import it into Alfred. Requires macOS and Alfred 5.5+ with Powerpack. Each build contains a native binary for one architecture only; Rust and Python are not needed to run it.

In **Configure Workflow…**, choose your screenshot folder. The initial folder is `~/Pictures/Screenshot`, matching your existing Screenshots workflow. The keyword is `shots`, so the old workflow can remain installed.

## Use

- `shots` → Return: browse the newest screenshots.
- `shots invoice` or `shots 2026-09`: search filenames and modification dates across the entire folder, then open the matching gallery. Multiple words may appear in any order.
- Select **Next page →** / **← Previous page** and press Return to browse more images.
- Return on an image opens Alfred's full-size image viewer. Escape returns to the gallery.
- Command + Return copies the original image to the clipboard.
- Option + Return reveals the original in Finder.
- Control + Return copies its file path.

The preview viewer also supports the copy/reveal shortcuts. Page navigation works with Return; modifiers on navigation tiles still change pages instead of passing them to a file action.

Search is entered in the `shots` keyword, before opening the gallery. The gallery has no local filter field: Alfred's native Grid View only filters the items it has already loaded, which would hide matches on other pages. Reopen `shots <query>` to search the whole folder.

## Why large folders stay responsive

- Only the current page's images are sent to Alfred: **48 by default**.
- Rust scans file metadata and retains only the newest entries needed for the requested page. The first page uses O(page size) retained file records, even when there are thousands of images.
- macOS ImageIO generates small previews directly from the source, with **at most four decoders** active. No per-file `mdls`, `sips`, or other subprocesses are launched.
- Previews are cached by source path, file size, modification/change timestamps, and preview size. Subsequent opens reuse them.
- Alfred receives cached PNG previews instead of thousands of original full-resolution images. The original is loaded only when you preview/copy that image.
- Search and paging rescan metadata, so added and removed files are reflected when opening a page. There is no dependency on Spotlight indexing or screenshot metadata tags.
- Corrupt or unsupported images get a fallback icon and remain selectable. Failed previews are retried after five minutes or when the source changes.

Images directly inside the selected folder are included: PNG, JPEG, WebP, HEIC/HEIF, TIFF, GIF, and BMP. Native decoding support depends on macOS. Animated images use their first frame. Subfolders, symlinks, hidden files, and videos are excluded. The source folder and its files are never modified.

This workflow browses local files. It does not upload images, run OCR, or access AI providers. Clipboard actions use Alfred's native image clipboard task.

## Settings

| Setting | Variable | Default |
| --- | --- | --- |
| Keyword | `SHOTS_KEYWORD` | `shots` |
| Screenshots folder | `SHOTS_FOLDER` | `~/Pictures/Screenshot` |
| Images per page | `SHOTS_PAGE_SIZE` | `48` (12–120) |
| Preview size | `SHOTS_PIXELS` | `384` pixels (longest edge) |

Previews are stored in this workflow's Alfred cache under `thumbnails-v1`, with directory mode 0700. Once a day on use, cached previews older than 30 days are removed and the cache is pruned to 512 MB. It may grow beyond that size between cleanups. Removing Alfred's workflow cache simply regenerates previews next time.

## Build and test

```sh
./scripts/cargo.sh test --locked
./build.sh
```

Builds use system Cargo when available, otherwise the existing local toolchain in `../alfred-quickai/.tools`. The package contains a signed Rust executable, a short shell launcher, native Alfred workflow metadata, icons, and documentation. Python is used only to build/verify the package.

Tests cover multi-page ordering and completeness, global search, Unicode and literal file paths, supported extensions, symlink exclusion, folder changes, corrupt images, native thumbnail generation, cache hits, and cache invalidation.

Run performance checks with an isolated cache:

```sh
/usr/bin/python3 -B scripts/benchmark.py
/usr/bin/python3 -B scripts/benchmark.py --folder ~/Pictures/Screenshot
```

The first command creates 25,000 temporary image paths (hard links to a generated 4K fixture). It verifies the first page, a warm cache, a deep page, and a search matching a file outside the first page. The second measures the real screenshot folder without changing source files. Benchmarks print only counts and timings.

Measured on this Apple Silicon Mac, 13 September 2026:

| Folder | First page, fresh previews | First page, cached previews | Last page |
| --- | --- | --- | --- |
| Actual screenshot folder: 2,896 images | 0.87 s | 0.18 s | 0.32 s |
| Synthetic folder: 25,000 image paths | 1.91 s | 1.53 s | 1.87 s |

These are CLI page-preparation times; Alfred's rendering time is excluded. Each page contained at most 48 image previews and the JSON stayed below 41 KB. All tested previews decoded successfully. The 25,000-file test reaches page 521 and verifies that global search finds a file beyond the first page. Cached preview generation itself took under 1 ms; large-folder warm times are mostly filesystem metadata scanning.

To inspect JSON directly:

```sh
SHOTS_FOLDER=~/Pictures/Screenshot SHOTS_STATS=1 ./workflow/shots browse ''
SHOTS_FOLDER=~/Pictures/Screenshot ./workflow/shots catalog '2026-09'
```

`SHOTS_STATS=1` emits counts and timing to stderr. `SHOTS_PAGE` is the zero-based page used internally. CLI arguments are passed literally, including spaces and shell characters.

Reference: [Alfred Grid View](https://www.alfredapp.com/help/workflows/user-interface/grid/).
