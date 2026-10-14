mod controller;
mod state;

use controller::Controller;
use serde_json::{json, Value};

fn items(running: bool, volume: f64, query: &str) -> Value {
    let status = if running { "Aktif" } else { "Berhenti" };
    let entries = [
        (
            "toggle",
            "Toggle iPhone Stream",
            format!(
                "{status} · Volume {:.0}% · Mulai/hentikan audio USB",
                volume * 100.0
            ),
        ),
        ("volume-up", "iPhone Volume Up", "Naikkan volume 10%".into()),
        (
            "volume-down",
            "iPhone Volume Down",
            "Turunkan volume 10%".into(),
        ),
        (
            "idam-toggle",
            "Toggle iPhone Audio MIDI",
            "Aktif/nonaktifkan Audio MIDI Mode pada iPhone yang terhubung".into(),
        ),
        (
            "audio-midi",
            "Open Audio MIDI Setup",
            "Aktifkan iPhone secara manual jika diperlukan".into(),
        ),
        (
            "log",
            "Open iPhone Stream Log",
            "Lihat pesan daemon untuk memeriksa masalah koneksi atau izin".into(),
        ),
    ];
    let query = query.trim().to_lowercase();
    let items: Vec<Value> = entries
        .into_iter()
        .filter(|(arg, title, _)| format!("{arg} {title}").to_lowercase().contains(&query))
        .map(|(arg, title, subtitle)| {
            json!({
                "uid": arg, "title": title, "subtitle": subtitle,
                "arg": arg, "valid": true, "icon": {"path": "icon.png"}
            })
        })
        .collect();
    json!({"items": items})
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("filter");
    let controller = Controller::from_env()?;
    if command == "filter" {
        let query = args
            .iter()
            .skip(1)
            .filter(|arg| arg.as_str() != "--")
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "{}",
            items(
                controller.running_pid().is_some(),
                state::read_volume(&controller.dir),
                &query
            )
        );
    } else if command == "status" {
        println!(
            "{}",
            json!({"running": controller.running_pid().is_some(), "volume": state::read_volume(&controller.dir)})
        );
    } else {
        if args.len() != 1 {
            return Err("Gunakan satu perintah tanpa argumen tambahan.".into());
        }
        println!("{}", controller.action(command)?);
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        // Alfred's notification receives errors as well as successful results.
        println!("iPhone Stream: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filter_exposes_all_original_commands_and_searches() {
        let all = items(true, 0.5, "");
        assert_eq!(all["items"].as_array().unwrap().len(), 6);
        assert!(all["items"][0]["subtitle"]
            .as_str()
            .unwrap()
            .contains("Aktif · Volume 50%"));
        let filtered = items(false, 1.0, "VOLUME");
        assert_eq!(filtered["items"].as_array().unwrap().len(), 2);
        assert_eq!(items(false, 1.0, "unknown")["items"], json!([]));
    }
}
