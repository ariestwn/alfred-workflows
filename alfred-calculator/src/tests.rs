use super::*;
use chrono::TimeZone;
use std::{collections::BTreeMap, fs};

fn fixture() -> (tempfile::TempDir, Config) {
    let temp = tempfile::tempdir().unwrap();
    let config = Config {
        comma: false,
        grouping: true,
        precision: 12,
        work_hours: 8,
        ppi: 96,
        timezone: "Asia/Jakarta".into(),
        cache: temp.path().into(),
        network: false,
        favorites: vec![
            "IDR".into(),
            "USD".into(),
            "EUR".into(),
            "GBP".into(),
            "BTC".into(),
        ],
    };
    for (name, values) in [
        (
            "fiat",
            vec![
                ("USD", 1.0),
                ("GBP", 0.8),
                ("EUR", 0.9),
                ("IDR", 16000.0),
                ("JPY", 150.0),
            ],
        ),
        (
            "crypto",
            vec![
                ("USD", 1.0),
                ("BTC", 0.00002),
                ("ETH", 0.0005),
                ("SOL", 0.01),
            ],
        ),
    ] {
        let snapshot = rates::Snapshot {
            fetched: rates::now(),
            as_of: rates::now(),
            rates: values
                .into_iter()
                .map(|(k, v)| (k.into(), v))
                .collect::<BTreeMap<_, _>>(),
        };
        fs::write(
            temp.path().join(format!("{name}.json")),
            serde_json::to_vec(&snapshot).unwrap(),
        )
        .unwrap();
    }
    (temp, config)
}
fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 13, 10, 0, 0).unwrap()
}
fn result(q: &str, config: &Config) -> Value {
    let result = filter(q, false, config, now());
    assert!(result["items"][0]["mods"].is_object(), "{q}: {result}");
    result
}
fn raw(q: &str, config: &Config) -> String {
    result(q, config)["items"][0]["mods"]["cmd"]["arg"]
        .as_str()
        .unwrap()
        .into()
}

#[test]
fn guide_math_and_unit_examples() {
    let (_temp, c) = fixture();
    for (q, expected) in [
        ("52% of 900", "468"),
        ("3% of $123", "3.69"),
        ("10ft in m", "3.048"),
        ("square root of 625", "25"),
        ("2 power 10", "1024"),
        ("145 mins to timespan", "8700"),
        ("55h in workdays", "6.875"),
        ("2 inches in px at 72 ppi", "144"),
        ("192 px in inches", "2"),
        ("20% off 80", "64"),
        ("15% tip on 42", "6.3"),
        ("ratio of 3 to 5", "3:5"),
        ("10K", "10000"),
        ("USD1K", "1000"),
        ("USD123", "123"),
        ("100 usd in gbp", "80"),
        ("1 BTC in USD", "50000"),
        ("10 SOL to usd", "1000"),
        ("$123 at 7% after 3 years", "150.680289"),
        ("0.1 + 0.2", "0.3"),
        ("32 F in C", "0"),
        ("2^100", "1267650600228229401496703205376"),
    ] {
        assert_eq!(raw(q, &c), expected, "{q}");
    }
    assert_eq!(
        result("15% tip on 42", &c)["items"][1]["mods"]["cmd"]["arg"],
        "48.3"
    );
}

#[test]
fn calculator_percent_operators_use_rates_and_relative_changes() {
    let (_temp, mut c) = fixture();
    for (q, expected) in [
        ("10000*10%", "1000"),
        ("10000 * 10%", "1000"),
        ("1000000-10%", "900000"),
        ("10000+10%", "11000"),
        ("10000/10%", "100000"),
        ("10%", "0.1"),
        ("10000*(10%)", "1000"),
        ("(10000-10%)*2", "18000"),
        ("10000-10%-5%", "8550"),
        ("10000-(5+5)%", "9000"),
        ("10000+10%*2", "10000.2"),
        ("10000-(10/100)", "9999.9"),
        ("10000*-10%", "-1000"),
        ("10000*10%+5%", "1050"),
        ("1e-2+10%", "0.011"),
        ("100 USD-10%", "90"),
        ("100 USD-10% in GBP", "72"),
        ("5%2", "1"),
        ("5 % 2", "1"),
        ("20% of 80", "16"),
        ("sin(30 degrees)%", "0.005"),
        ("0.1 to %", "10"),
    ] {
        assert_eq!(raw(q, &c), expected, "{q}");
    }
    assert_eq!(result("10000*10%", &c)["items"][0]["title"], "1,000");
    assert_eq!(
        result("1000000-10%", &c)["items"][0]["text"]["largetype"],
        "1000000-10% = 900,000"
    );
    c.comma = true;
    assert_eq!(raw("1.000-10,5%", &c), "895");
    assert_eq!(raw("10.000*10%", &c), "1000");
}

#[test]
fn advanced_trigonometry_and_domain_errors() {
    let (_temp, c) = fixture();
    for (q, expected) in [
        ("sin(90 degrees)", 1.0),
        ("acos(1)", 0.0),
        ("cot(1)", 1.0 / 1f64.tan()),
        ("csc(1)", 1.0 / 1f64.sin()),
        ("sinh(1)", 1f64.sinh()),
        ("acot(1)", std::f64::consts::FRAC_PI_4),
        ("coth(1)", 1.0 / 1f64.tanh()),
        ("acsch(1)", 1f64.asinh()),
    ] {
        assert!(
            (raw(q, &c).parse::<f64>().unwrap() - expected).abs() < 1e-10,
            "{q}"
        );
    }
    for q in [
        "1/0",
        "2 metres in kg",
        "2 inches in px at 0 ppi",
        "ratio of 3 to 0",
    ] {
        assert_eq!(
            filter(q, false, &c, now())["items"][0]["valid"],
            false,
            "{q}"
        );
    }
}

#[test]
fn system_style_override_and_exact_large_integer_copy() {
    let (_temp, mut c) = fixture();
    c.comma = true;
    assert_eq!(raw("1.234,5 + 0,5", &c), "1235");
    assert_eq!(result("1.234,5 + 0,5", &c)["items"][0]["title"], "1.235");
    assert_eq!(raw("1,5 usd in gbp", &c), "1.2");
    c.grouping = false;
    assert_eq!(result("1.234,5 + 0,5", &c)["items"][0]["title"], "1235");
    assert_eq!(
        c.format("1267650600228229401496703205376"),
        "1267650600228229401496703205376"
    );
}

#[test]
fn decimal_precision_rounds_answers_and_keeps_large_integers() {
    let (_temp, mut c) = fixture();
    for (precision, expected, expected_raw) in [
        (0, "≈ 1,235", "1235"),
        (1, "≈ 1,234.6", "1234.6"),
        (2, "≈ 1,234.57", "1234.57"),
        (3, "1,234.567", "1234.567"),
    ] {
        c.precision = precision;
        let output = result("1234.567", &c);
        let item = &output["items"][0];
        assert_eq!(item["title"], expected);
        assert_eq!(item["arg"], expected);
        assert_eq!(item["text"]["copy"], expected);
        assert_eq!(item["mods"]["alt"]["arg"], expected);
        assert_eq!(item["mods"]["cmd"]["arg"], expected_raw);
        assert_eq!(raw("2^100", &c), "1267650600228229401496703205376");
    }
    c.precision = 0;
    assert_eq!(result("-1234.567", &c)["items"][0]["title"], "≈ −1,235");
    assert_eq!(result("999.9", &c)["items"][0]["title"], "≈ 1,000");
    assert_eq!(result("10ft in m", &c)["items"][0]["title"], "≈ 3 m");
    let fx = filter("1.234 USD GBP", true, &c, now());
    assert_eq!(fx["items"][0]["title"], "≈ 1 GBP");
    c.precision = 2;
    assert_eq!(result("1.5", &c)["items"][0]["title"], "1.5");
    c.comma = true;
    assert_eq!(result("1.234,567", &c)["items"][0]["title"], "≈ 1.234,57");
    c.grouping = false;
    assert_eq!(result("1.234,567", &c)["items"][0]["title"], "≈ 1234,57");
}

#[test]
fn shift_return_copies_rounded_numbers_with_units_and_locale() {
    let (_temp, mut c) = fixture();
    for (query, expected) in [
        ("177904.72955 IDR", "177,905 IDR"),
        ("1234.4", "1,234"),
        ("999.9", "1,000"),
        ("-1234.6", "−1,235"),
        ("-0.1", "0"),
        ("2.5", "3"),
        ("-2.5", "−3"),
        ("10ft in m", "3 m"),
        ("9007199254740993.6", "9,007,199,254,740,994"),
        ("2^100", "1,267,650,600,228,229,401,496,703,205,376"),
        ("1.234 USD to GBP", "1 GBP"),
    ] {
        let output = result(query, &c);
        let item = &output["items"][0];
        assert_eq!(item["mods"]["shift"]["arg"], expected, "{query}: {output}");
        assert_eq!(
            item["mods"]["shift"]["subtitle"],
            format!("Copy rounded: {expected}")
        );
        assert_eq!(item["mods"]["shift"]["variables"]["CALC_ACTION"], "copy");
        assert_eq!(item["arg"], item["text"]["copy"]);
        assert_eq!(item["mods"]["alt"]["arg"], item["arg"]);
        assert!(item["subtitle"].as_str().unwrap().contains("⇧↩ Round"));
    }
    let output = result("177904.72955 IDR", &c);
    assert_eq!(output["items"][0]["arg"], "177,904.72955 IDR");
    assert_eq!(output["items"][0]["mods"]["cmd"]["arg"], "177904.72955");
    let fx = filter("1.234 USD GBP", true, &c, now());
    assert_eq!(fx["items"][0]["mods"]["shift"]["arg"], "1 GBP");
    for query in [
        "today",
        "monday in 3 weeks",
        "now",
        "145 mins to timespan",
        "ratio of 3 to 5",
        "sqrt(-1)",
    ] {
        let output = result(query, &c);
        assert!(
            output["items"][0]["mods"].get("shift").is_none(),
            "{query}: {output}"
        );
    }
    c.comma = true;
    assert_eq!(
        result("177.904,72955 IDR", &c)["items"][0]["mods"]["shift"]["arg"],
        "177.905 IDR"
    );
    c.grouping = false;
    assert_eq!(
        result("177904,72955 IDR", &c)["items"][0]["mods"]["shift"]["arg"],
        "177905 IDR"
    );
}

#[test]
fn zero_precision_preserves_timespans_and_integer_trailing_zeroes() {
    let (_temp, mut c) = fixture();
    c.precision = 0;
    assert_eq!(raw("145 mins to timespan", &c), "8700");
    assert_eq!(raw("today", &c), "2026-09-13");
    assert_eq!(math::decimal(100.0, 0), "100");
    assert_eq!(math::decimal(-10.0, 0), "-10");
    assert_eq!(math::decimal(-0.1, 0), "0");
    let duration = result("90.4 seconds to timespan", &c);
    assert_eq!(duration["items"][0]["title"], "1 minute 30.4 seconds");
    assert_eq!(duration["items"][0]["mods"]["cmd"]["arg"], "90");
}

#[test]
fn calendar_and_time_examples() {
    let (_temp, c) = fixture();
    for (q, expected) in [
        ("today", "2026-09-13"),
        ("tomorrow", "2026-09-14"),
        ("yesterday", "2026-09-12"),
        ("monday in 3 weeks", "2026-10-05"),
        ("days until 31 Mar", "199"),
        ("workhours in 2023", "2080"),
        ("August 5 + 5", "2026-08-10"),
        ("2024-02-28 + 1", "2024-02-29"),
        ("diff Paris", "-5"),
        ("2024-03-15T14:30:00Z", "2024-03-15T21:30:00+07:00"),
        ("5pm ldn in sf", "2026-09-13T09:00:00-07:00"),
        ("3:45pm + 5", "2026-09-13T20:45:00+07:00"),
        ("time in São Paulo", "2026-09-13T07:00:00-03:00"),
        ("time in JFK", "2026-09-13T06:00:00-04:00"),
        (
            "time in 4 hours in San Francisco",
            "2026-09-13T07:00:00-07:00",
        ),
    ] {
        assert_eq!(raw(q, &c), expected, "{q}");
    }
}

#[test]
fn days_since_resolves_past_dates_and_copy_payloads() {
    let (_temp, c) = fixture();
    for (query, expected) in [
        ("days since 21 September", "357"),
        ("day since 21 September to today", "357"),
        ("DAYS  SINCE  September  21st  TO TODAY", "357"),
        ("days since 21 Sep 2024", "722"),
        ("days since 2025-09-21", "357"),
        ("days since September 1", "12"),
        ("days since 13 Sep", "0"),
        ("days since today", "0"),
        ("days since yesterday to today", "1"),
        ("days since 31 Dec", "256"),
    ] {
        assert_eq!(raw(query, &c), expected, "{query}");
    }
    let item = &result("days since 21 Sep", &c)["items"][0];
    assert_eq!(item["title"], "357 days");
    assert_eq!(item["arg"], "357 days");
    assert_eq!(item["text"]["copy"], "357 days");
    assert_eq!(item["text"]["largetype"], "days since 21 Sep = 357 days");
    assert!(item["subtitle"]
        .as_str()
        .unwrap()
        .contains("2025-09-21 → 2026-09-13"));
    assert_eq!(
        result("days since yesterday", &c)["items"][0]["title"],
        "1 day"
    );
    for query in [
        "days since",
        "day since ",
        "days since 31 Feb",
        "days since 2025-02-29",
        "days since nonsense",
        "days since 21 Sep 2027",
        "days since tomorrow",
        "days since 21 Sep to tomorrow",
    ] {
        assert_eq!(
            filter(query, false, &c, now())["items"][0]["valid"],
            false,
            "{query}"
        );
    }
}

#[test]
fn days_since_handles_leap_days_local_midnight_and_dst() {
    let (_temp, mut c) = fixture();
    for (instant, query, start, expected) in [
        ((2026, 9, 21, 0), "days since 21 Sep", "2026-09-21", "0"),
        (
            (2026, 9, 21, 0),
            "days since 21 Sep 2025",
            "2025-09-21",
            "365",
        ),
        ((2025, 3, 1, 0), "days since 29 Feb", "2024-02-29", "366"),
        ((2024, 2, 28, 0), "days since 29 Feb", "2020-02-29", "1460"),
        ((2024, 2, 29, 0), "days since 29 Feb", "2024-02-29", "0"),
        ((2104, 2, 28, 0), "days since 29 Feb", "2096-02-29", "2920"),
        ((2026, 9, 20, 18), "days since 21 Sep", "2026-09-21", "0"),
    ] {
        let (year, month, day, hour) = instant;
        let at = Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).unwrap();
        let output = filter(query, false, &c, at);
        let item = &output["items"][0];
        assert_eq!(item["mods"]["cmd"]["arg"], expected, "{query}: {output}");
        assert!(item["subtitle"].as_str().unwrap().starts_with(start));
    }
    c.timezone = "America/New_York".into();
    for (month, day, hour, start) in [(3, 9, 4, "2026-03-08"), (11, 2, 5, "2026-11-01")] {
        let at = Utc.with_ymd_and_hms(2026, month, day, hour, 0, 0).unwrap();
        let output = filter(&format!("days since {start}"), false, &c, at);
        assert_eq!(output["items"][0]["mods"]["cmd"]["arg"], "1");
    }
}

#[test]
fn indonesian_places_use_their_regional_timezones() {
    let (_temp, mut c) = fixture();
    for (place, expected, abbreviation) in [
        ("balige", "2026-09-13T17:00:00+07:00", "WIB"),
        ("MEDAN", "2026-09-13T17:00:00+07:00", "WIB"),
        ("Danau  Toba", "2026-09-13T17:00:00+07:00", "WIB"),
        ("samosir", "2026-09-13T17:00:00+07:00", "WIB"),
        ("siborong-borong", "2026-09-13T17:00:00+07:00", "WIB"),
        ("pematang siantar", "2026-09-13T17:00:00+07:00", "WIB"),
        ("sumatera utara", "2026-09-13T17:00:00+07:00", "WIB"),
        ("bandung", "2026-09-13T17:00:00+07:00", "WIB"),
        ("pontianak", "2026-09-13T17:00:00+07:00", "WIB"),
        ("Palangka_Raya", "2026-09-13T17:00:00+07:00", "WIB"),
        ("balikpapan", "2026-09-13T18:00:00+08:00", "WITA"),
        ("banjarmasin", "2026-09-13T18:00:00+08:00", "WITA"),
        ("tarakan", "2026-09-13T18:00:00+08:00", "WITA"),
        ("makassar", "2026-09-13T18:00:00+08:00", "WITA"),
        ("manado", "2026-09-13T18:00:00+08:00", "WITA"),
        ("bali", "2026-09-13T18:00:00+08:00", "WITA"),
        ("labuan bajo", "2026-09-13T18:00:00+08:00", "WITA"),
        ("kupang", "2026-09-13T18:00:00+08:00", "WITA"),
        ("ambon", "2026-09-13T19:00:00+09:00", "WIT"),
        ("ternate", "2026-09-13T19:00:00+09:00", "WIT"),
        ("sorong", "2026-09-13T19:00:00+09:00", "WIT"),
        ("merauke", "2026-09-13T19:00:00+09:00", "WIT"),
    ] {
        let query = format!("time in {place}");
        let output = result(&query, &c);
        assert_eq!(
            output["items"][0]["mods"]["cmd"]["arg"], expected,
            "{query}"
        );
        assert!(
            output["items"][0]["title"]
                .as_str()
                .unwrap()
                .ends_with(abbreviation),
            "{query}: {output}"
        );
    }
    for (query, expected) in [
        ("5pm medan in tokyo", "2026-09-13T19:00:00+09:00"),
        ("5pm balige in bali", "2026-09-13T18:00:00+08:00"),
        ("5pm tokyo in medan", "2026-09-13T15:00:00+07:00"),
        ("11pm medan in jayapura", "2026-09-14T01:00:00+09:00"),
        ("time in 4 hours in balige", "2026-09-13T21:00:00+07:00"),
        ("diff medan", "0"),
        ("time diff balige", "0"),
        ("diff makassar", "1"),
        ("diff sorong", "2"),
    ] {
        assert_eq!(raw(query, &c), expected, "{query}");
    }
    // A region spanning multiple zones must not silently pick one.
    assert_eq!(
        filter("time in kalimantan", false, &c, now())["items"][0]["valid"],
        false
    );
    c.timezone = "Medan".into();
    assert_eq!(raw("now", &c), "2026-09-13T17:00:00+07:00");
}

#[test]
fn dst_gaps_and_repeated_clock_times_are_explicit() {
    let (_temp, c) = fixture();
    let gap = filter("2026-03-08 2:30am New York in London", false, &c, now());
    assert_eq!(gap["items"][0]["valid"], false);
    assert!(gap.to_string().contains("does not exist"));
    let fold = result("2026-11-01 1:30am New York in London", &c);
    assert_eq!(fold["items"].as_array().unwrap().len(), 2);
    assert_ne!(fold["items"][0]["arg"], fold["items"][1]["arg"]);
    assert!(fold.to_string().contains("ambiguous clock time"));
    assert_eq!(
        filter("2024-02-30", false, &c, now())["items"][0]["valid"],
        false
    );
    assert!(dates::Zone::parse("CST").is_err());
}

#[test]
fn currency_browser_completion_names_optional_connectors_and_shortcuts() {
    let (_temp, c) = fixture();
    let all = filter("100", true, &c, now());
    assert_eq!(all["items"][0]["autocomplete"], "100 IDR to ");
    assert_eq!(all["items"][0]["valid"], false);
    for q in [
        "100 usd to gbp",
        "100 usd in gbp",
        "100 usd as gbp",
        "100 usd gbp",
        "100 United States Dollar to United Kingdom Pound",
    ] {
        let result = filter(q, true, &c, now());
        assert_eq!(result["items"][0]["title"], "80 GBP", "{q}: {result}");
        assert_eq!(result["items"][0]["mods"]["cmd"]["arg"], "80");
        assert_eq!(
            result["items"][0]["mods"]["alt"]["variables"]["CALC_ACTION"],
            "paste"
        );
        assert_eq!(
            result["items"][0]["text"]["largetype"],
            "100 USD to GBP = 80 GBP"
        );
    }
    assert!(
        filter("100 dol", true, &c, now())["items"]
            .as_array()
            .unwrap()
            .len()
            > 1
    );
    let words = result("2+2", &c);
    assert!(words["items"][0]["mods"].get("cmd+shift").is_none());
    assert_eq!(words["items"][0]["text"]["largetype"], "2+2 = 4");
    let big = filter("9007199254740993 USD USD", true, &c, now());
    assert_eq!(big["items"][0]["mods"]["cmd"]["arg"], "9007199254740993");
}

#[test]
fn invalid_rates_are_rejected_and_stale_cache_is_labeled() {
    let (temp, c) = fixture();
    let bad = json!({"result":"success","base_code":"EUR","time_last_update_unix":rates::now(),"rates":{"USD":1,"GBP":0.8}});
    assert!(rates::parse("fiat", &bad, rates::now()).is_err());
    let zero = json!({"data":{"currency":"USD","rates":{"USD":"1","BTC":"0"}}});
    assert!(rates::parse("crypto", &zero, rates::now()).is_err());
    let path = temp.path().join("fiat.json");
    let mut cache: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    cache["fetched"] = (rates::now() - 100000).into();
    fs::write(&path, serde_json::to_vec(&cache).unwrap()).unwrap();
    let result = result("100 usd in gbp", &c);
    assert!(result["items"][0]["subtitle"]
        .as_str()
        .unwrap()
        .contains("Cached / stale"));
    fs::remove_file(path).unwrap();
    let failed = filter("100 usd in gbp", false, &c, now());
    assert_eq!(failed["items"][0]["valid"], false);
    assert!(failed.get("rerun").is_none());
}

#[test]
fn expensive_or_malformed_input_is_bounded_and_never_actionable() {
    let (_temp, c) = fixture();
    for q in [
        "(".repeat(65) + "2" + &")".repeat(65),
        "a".repeat(2050),
        "$(touch /tmp/should-not-exist)".into(),
        "2 +".into(),
    ] {
        assert_eq!(filter(&q, false, &c, now())["items"][0]["valid"], false);
    }
    assert!(!std::path::Path::new("/tmp/should-not-exist").exists());
}
