# Calculator Rust for Alfred

Math, units, fiat and crypto conversions, dates, and time zones in an Alfred workflow. Inspired by the supplied Raycast Calculator guide and Vítor Galvão's Alfred Currency Converter workflow. The calculation engine and workflow logic run in Rust.

## Install

Download the Apple Silicon or Intel build from the [releases page](https://github.com/ariestwn/alfred-workflows/releases) and open it to import into Alfred. Requires Alfred 5 with Powerpack and macOS. Each build contains a native binary for one architecture only. Rust, Python, Node, and Raycast are not needed at runtime.

Use `calc <expression>`. Type `calc` alone to see examples; Return on an example fills it into the input. Use `fx` for the currency browser. These are workflow keywords; Alfred's built-in root calculator remains separate. Alfred Script Filter rows display the answer and explanation as plain text, without Raycast's split expression/answer layout or syntax highlighting.

| Input | What it does |
| --- | --- |
| `calc (20 + 5) * 4` | Arithmetic, parentheses, powers, factorials, constants |
| `calc 52% of 900` | Percentage: 468 |
| `calc 10000*10%` | Multiply by a rate: 1,000 |
| `calc 1000000-10%` | Reduce the amount by 10%: 900,000 |
| `calc 10000+10%` | Increase the amount by 10%: 11,000 |
| `calc 10000/10%` | Divide by a rate: 100,000 |
| `calc square root of 625` | Square root: 25 |
| `calc 2 power 10` | Power: 1,024 |
| `calc 10ft in m` | Unit conversion: 3.048 m |
| `calc 100 usd in gbp` | Currency conversion |
| `calc 0.1 BTC in IDR` | Crypto conversion |
| `calc 20% off 80` | Discounted price: 64 |
| `calc 15% tip on 42` | Tip: 6.3; a second row shows total 48.3 |
| `calc ratio of 3 to 5` | Simplified ratio and decimal |
| `calc USD1K` / `calc 10K` | Currency and numeric shorthand |
| `calc $123 at 7% after 3 years` | Annual compound growth, no contributions or fees |
| `calc 145 mins to timespan` | 2 hours 25 minutes |
| `calc 55h in workdays` | 6.875 workdays at the default 8 hours/day |
| `calc workhours in 2023` | 2,080 Monday–Friday workhours |
| `calc 2 inches in px at 72 ppi` | 144 pixels |
| `calc cot(1)` / `calc acos(1)` | Trigonometry; bare angles are radians |
| `calc sin(90 degrees)` | Explicit degree input |
| `calc 5pm ldn in sf` | London to San Francisco, with date and DST |
| `calc time in tokyo` / `calc time in JFK` | Time in a city or airport alias |
| `calc time in São Paulo` | Multi-word and accented city alias |
| `calc time in balige` / `calc time in medan` | Current time in WIB |
| `calc 5pm medan in tokyo` | 17:00 in Medan is 19:00 in Tokyo |
| `calc 5pm balige in bali` | 17:00 in Balige is 18:00 in Bali |
| `calc time diff Paris` / `calc diff Paris` | Offset difference relative to your local timezone |
| `calc time in 4 hours in San Francisco` | Future time in a destination |
| `calc monday in 3 weeks` | Upcoming weekday relative to a future date |
| `calc days until 31 Mar` | Calendar-day countdown |
| `calc days since 21 Sep 2025` | Calendar days from a past date to today |
| `calc days since 21 Sep` | Days since the most recent 21 September, including today |
| `calc day since 21 September to today` | Same calculation; singular `day` and `to today` are optional forms |
| `calc days until end of quarter` | Days until the final date of this quarter |
| `calc 2024-03-15T14:30:00Z` | ISO timestamp in your local timezone |
| `calc August 5 + 5` | Add 5 calendar days |
| `calc 3:45pm + 5` | Add 5 elapsed hours |
| `calc now`, `today`, `tomorrow`, `yesterday`, `time` | Current/relative date or time |

Math is powered by [fend-core](https://printfn.github.io/fend/documentation/), which also supports complex numbers, fractions, scientific units, logarithms, and bitwise operations. Additional reciprocal and inverse functions include `cot`, `csc`, `sec`, `coth`, `csch`, `sech`, `acot`, `acsc`, `asec`, `acoth`, `acsch`, and `asech`. The natural-language layer supports the forms documented here; it is not a general-language AI parser.

## Copy and paste

Result subtitles show shortcuts and only essential context, such as date ranges, timezones, rate status, and labels distinguishing multiple answers.

Percent operators follow calculator conventions: `a + b%` and `a - b%` increase or decrease the preceding amount by that rate. Chained changes apply left to right (`100 - 10% - 5%` is 85.5). Multiplication and division use the rate as a fraction (`10%` is `0.1`). Normal operator precedence still applies: `100 + 10% * 2` is `100.2`. To subtract an absolute decimal, write `100 - 0.1`. `5 % 2` remains modulo, and explicit conversions such as `0.1 to %` still display percentages.

- Return: copy the formatted answer, including its unit when present, and close Alfred.
- Shift-Return: copy a numeric result rounded to the nearest whole number, preserving grouping and units (for example, `177,904.72955 IDR` becomes `177,905 IDR`). Hold Shift to preview the rounded copy value. Available in both `calc` and `fx`; dates, times, and multi-part durations keep their normal shortcuts.
- Command-Return: copy the unformatted answer, without grouping or units and with a dot decimal separator. Dates/times use ISO format; a timespan uses seconds; a time difference uses hours.
- Option-Return: paste the formatted answer into the active app.
- Command-C: copy the formatted result. Command-L: show question and answer in Large Type.

Two text Universal Actions and an unassigned calculator hotkey are included. Automatic paste uses Alfred's normal Accessibility permission. Invalid and incomplete expressions are not actionable.

## Currency browser

Type `fx 100` to choose a source. Type `fx 100 dol` to filter names, or `fx 100 USD` to see target conversions. Narrow targets with `fx 100 USD to GBP` or `fx 100 USD to rupiah`. `to`, `in`, and `as` are optional: `fx 100 USD GBP` also works. Return on a source suggestion completes it; Return on a result copies the answer.

Favorites appear first, with IDR, USD, EUR, GBP, JPY, SGD, AUD, BTC, and ETH as defaults. The names list includes 166 fiat codes and 18 crypto codes; availability depends on the provider, and obsolete/unavailable codes do not produce invented rates.

Fiat rates are provided by [ExchangeRate-API](https://www.exchangerate-api.com), which updates its open feed daily. Crypto rates come from [Coinbase](https://docs.cdp.coinbase.com/coinbase-app/track-apis/exchange-rates). No API keys are needed. Fiat data is refreshed after 12 hours; crypto after 5 minutes. Downloads happen in the background, and Alfred refreshes the results when data arrives. The workflow only requests fixed USD-based rate tables; it does not send your expression, amount, or history to either service.

Results show the source and UTC rate timestamp. When refresh fails, previous rates stay usable and are labeled **Cached / stale**. If no usable cache exists, the workflow shows an unavailable/loading result instead of making up a conversion. Failed downloads wait a minute before retrying. Exchange rates are indicative; they do not include provider fees or buy/sell spreads. Cached data is stored under Alfred's workflow cache and is not bundled in exports.

## Dates and conventions

- The local timezone follows macOS by default, with an IANA-name override in Workflow Configuration. Named zones apply daylight-saving rules from the bundled timezone database.
- City aliases include London/LDN, San Francisco/SF, New York/NYC/JFK, Tokyo/NRT/HND, São Paulo/GRU, Jakarta/CGK, Bali/DPS, Singapore/SIN, and others. Full IANA names such as `America/New_York` also work. Airport aliases are curated, not an exhaustive airport database.
- Indonesian aliases include Balige, Medan, Toba/Danau Toba, Samosir, Parapat, Tarutung, Pematangsiantar/Siantar, and cities and provinces across WIB, WITA, and WIT. For example, `time in palangka raya`, `time in balikpapan`, `time in ambon`, and `diff medan` work. Names ignore case, repeated whitespace, and underscores. Lookups work offline using the curated list in `data/places.json`; it does not include every district or village. Unlisted places can use `WIB`, `WITA`, or `WIT` explicitly.
- Indonesian timezone assignments follow [BMKG's regional time zones](https://www.bmkg.go.id/tanda-waktu). Sumatra and Java use `Asia/Jakarta`, western/central Kalimantan uses `Asia/Pontianak`, central Indonesia uses `Asia/Makassar`, and Maluku/Papua uses `Asia/Jayapura`. The result subtitle shows the timezone's IANA name, which may differ from the city requested.
- `CST`, `IST`, and `BST` are treated as ambiguous. Use a city name. Explicit `PST`, `PDT`, `EST`, and `EDT` are fixed offsets; `PT` and `ET` follow US daylight saving. Fixed offsets such as `UTC+05:30` work.
- A clock conversion uses today's date in the source zone. Include a date to schedule precisely: `2026-11-01 1:30am New York in London`. Repeated DST times return both possibilities; nonexistent local times produce an error.
- `monday in 3 weeks` chooses the first Monday on or after today plus 3 weeks. A month/day without a year uses this year; a `days until` query uses its next occurrence when it has already passed. Include the year when it matters.
- `days since DATE` counts elapsed calendar days in your configured local timezone, without counting the start date (today is 0, yesterday is 1). `day since DATE` and an optional `to today` suffix also work. A month/day without a year chooses its most recent occurrence on or before today; `29 Feb` finds the most recent leap day. Include a year to select an older occurrence, for example `days since 21 Sep 2025` or `days since 2025-09-21`. The result shows both dates. Explicit future dates are rejected; use `days until` for a countdown.
- Work counts use Monday–Friday and the configured hours per day. They include public holidays and do not access your calendar. `55h in workdays` converts duration, rather than counting dates.
- Date-only addition defaults to calendar days. Clock addition defaults to elapsed hours. All dates use the Gregorian calendar.

## Configuration

Workflow Configuration controls both keywords, number format, digit grouping, maximum fractional digits, local timezone, workday hours, default PPI, favorite currencies, and offline mode. Grouped inputs follow the chosen number format. Bare `10K`, `10M`, and `10B` mean thousand, million, and billion; use a space for a Kelvin unit (`10 K`). Calculations use a fresh context per query, so variables do not persist across searches.

In Alfred Preferences → Workflows → Calculator Rust → Configure Workflow, choose **Number format**:

- **Local (follow macOS)**: use the decimal style from your Mac's region settings.
- **United States — 1,234.56**: comma for thousands, dot for decimals.
- **Indonesia — 1.234,56**: dot for thousands, comma for decimals.

The format applies to typed numbers, displayed results, normal copy/paste, and Shift-Return rounded copy. For example, `177904.72955 IDR` in United States format rounds to `177,905 IDR`; `177904,72955 IDR` in Indonesia format rounds to `177.905 IDR`. **Thousands separators** controls digit grouping separately, and **Decimal precision** controls how many decimal places are shown. Command-Return always copies a plain number with a dot decimal and no grouping. Number format does not change date language, timezone, or currency. Existing dot/comma preferences are preserved when updating.

In Alfred Preferences → Workflows → Calculator Rust → Configure Workflow, set **Decimal precision** to any value from **0 to 30** (default: 12). Choose **0 (no decimals)** for whole numbers, **2** for up to two decimal places, or another count as needed. For example, `1234.567` displays `≈ 1,235` at 0, `≈ 1,234.57` at 2, and `1,234.567` at 3 when using dot decimals and grouping. Results are rounded, and exact short decimals are not padded (`1.5` stays `1.5` at 2). The setting also applies to raw numeric copy; it does not reduce integer precision or change date/time formats.

## Build and test

```sh
./scripts/cargo.sh fetch --locked  # once, with network access
./scripts/cargo.sh test --locked --offline
./scripts/cargo.sh clippy --locked --offline --all-targets -- -D warnings
./build.sh
```

Requires Rust, Xcode command line tools, and Python 3 for packaging. `scripts/cargo.sh` uses Cargo from PATH, falling back to the sibling `alfred-quickai/.tools` toolchain in this workspace. `build.sh` builds and signs the native binary, generates the icon if missing, and packages the workflow. Runtime network requests use macOS's `/usr/bin/curl` with HTTPS, size limits, and timeouts; all calculation, parsing, caching, and result generation run in Rust.

Tests use deterministic dates and temporary rate fixtures, with network disabled. They cover the guide examples, large integers, decimal styles, units, currency completion, clipboard payloads, leap dates, and DST transitions. `scripts/verify_workflow.py` validates the archive and workflow graph and exercises the packaged executable. Expressions are limited to 2,048 bytes and 64 nested parentheses; math evaluation has a 400 ms interruption deadline.

The supplied Raycast guide and existing Currency Converter workflow informed this implementation. Currency names are factual metadata; no existing workflow scripts or icons are bundled. Third-party library notices are included under `licenses/`.
