# Third-party notices

Calculation engine: fend-core 1.5.8, MIT, https://github.com/printfn/fend

Date/time libraries: chrono (MIT OR Apache-2.0), chrono-tz (MIT OR Apache-2.0),
and iana-time-zone (MIT OR Apache-2.0). chrono-tz contains generated timezone
rules from the IANA timezone database. Regex parsing uses regex (MIT OR
Apache-2.0). Serialization uses serde/serde_json (MIT OR Apache-2.0).
Temporary-file management uses tempfile (MIT OR Apache-2.0), with libc and
the transitive dependencies recorded in Cargo.lock.

License and copyright files from the resolved crates are included in
`licenses/`. macOS frameworks, system curl, Alfred, and Raycast are not bundled.

The user-provided Raycast Calculator guide and Vítor Galvão's Alfred Currency
Converter (com.alfredapp.vitor.currencyconverter) were used as behavioral
references. The fiat currency-name metadata was transcribed from the supplied
workflow. All workflow Rust code, packaging code, and the calculator icon are
new for this project.

Live fiat rates: https://www.exchangerate-api.com
Live crypto rates: https://docs.cdp.coinbase.com/coinbase-app/track-apis/exchange-rates

No provider exchange-rate data is included in the workflow archive.
