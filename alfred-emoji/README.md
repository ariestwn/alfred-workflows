# Emoji

A Raycast-style emoji picker for Alfred, built in Rust. It shows emoji in Alfred's native Grid View and adds semantic search powered by [TypeSafe Jev](https://docs.typesafe.ai).

## Install

Download the Apple Silicon or Intel build from the [releases page](https://github.com/ariestwn/alfred-workflows/releases) and open it. Requires Alfred 5.5+ with Powerpack.

## Use

- `emoji` → Return: browse all 1,923 emoji, four rows at a time. Select **Next** or **Previous** and press Return to change page.
- `emoji happy` → Return: search by name or CLDR keyword (`happy`, `lol`, `money`). Works offline.
- `emoji i got promoted`: also searches by meaning with Jev when an API key is set. Exact names come first, then Jev's ranking, then keyword matches.

Alfred's Grid View has no row setting and grows with the number of items, so each page holds 30 emoji plus up to two page tiles. Search happens before the grid opens because Alfred's grid filter only sees the current page.
- Return pastes the emoji into the front app. ⌘↩ copies without pasting. ⌥↩ copies the emoji's name.

## Semantic search

Set **TypeSafe API key** in Configure Workflow. Without one, `emoji <query>` uses keyword search only.

Jev never generates emoji. It picks from the catalog in two steps, following TypeSafe's hierarchical classification pattern:

1. Two parallel Choice questions over the 98 Unicode subgroups (for example, `face-hat` or `event`): one reads the query literally, the other as chat slang, innuendo, or meme usage. The top 3 subgroups from each are kept. The oversized `country-flag` subgroup is split in two so each question stays under the 255-option limit.
2. Parallel Choice questions over the emoji in those subgroups. Each also has a "none" option.

Each emoji's score is `sqrt(p_subgroup × p_emoji)`. Scores below 0.05 are dropped, and up to 24 results are kept. Results are cached per query in Alfred's workflow cache (`jev-v1/`), so repeating a query is instant and free.

## Settings

| Setting | Variable | Default |
| --- | --- | --- |
| Keyword | `EMOJI_KEYWORD` | `emoji` |
| TypeSafe API key | `TYPESAFE_API_KEY` | none |
| Jev model | `JEV_MODEL` | `jev-latest` |

## Build

```sh
./scripts/cargo.sh test
./build.sh
```

Data comes from Unicode `emoji-test.txt` and CLDR English annotations. Skin-tone variants are omitted. Run `python3 scripts/prepare_data.py` to regenerate `data/emoji.tsv`, and delete `workflow/icons` to re-render tiles with Apple Color Emoji.
