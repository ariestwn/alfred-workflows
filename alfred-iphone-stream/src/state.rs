use std::{fs, path::Path};

pub fn clamp(volume: f64) -> f64 {
    if !volume.is_finite() {
        return 1.0;
    }
    (volume.clamp(0.0, 1.0) * 100.0).round() / 100.0
}

pub fn read_volume(dir: &Path) -> f64 {
    fs::read(dir.join("state.json"))
        .ok()
        .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok())
        .and_then(|state| state["volume"].as_f64())
        .map(clamp)
        .unwrap_or(1.0)
}

// Callers hold the command lock; rename prevents partial reads by the daemon.
pub fn write_volume(dir: &Path, volume: f64) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let temp = dir.join("state.json.tmp");
    fs::write(
        &temp,
        serde_json::json!({"volume": clamp(volume)}).to_string(),
    )
    .map_err(|e| e.to_string())?;
    fs::rename(temp, dir.join("state.json")).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn volume_handles_bounds_and_invalid_state() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_volume(dir.path()), 1.0);
        fs::write(dir.path().join("state.json"), "broken").unwrap();
        assert_eq!(read_volume(dir.path()), 1.0);
        for (input, expected) in [(-0.1, 0.0), (1.1, 1.0), (0.29999999, 0.3), (f64::NAN, 1.0)] {
            write_volume(dir.path(), input).unwrap();
            assert_eq!(read_volume(dir.path()), expected);
        }
    }
}
