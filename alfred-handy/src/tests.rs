use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};

fn fixture() -> (tempfile::TempDir, Paths, Db) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths {
        data: dir.path().join("data"),
        hub: dir.path().join("hub"),
        binary: dir.path().join("Handy.app/Contents/MacOS/handy"),
        trash: dir.path().join("trash"),
    };
    fs::create_dir_all(paths.data.join("recordings")).unwrap();
    let db = Db::fixture(&paths.db());
    (dir, paths, db)
}
fn insert(db: &Db, id: i64, original: &str, processed: Option<&str>) {
    db.query("INSERT INTO transcription_history (id,file_name,timestamp,title,transcription_text,post_processed_text) VALUES (?,?,?,'Example',?,NULL)",
        &[id.to_string(),format!("{id}.wav"),"12345".into(),original.into()]).unwrap();
    if let Some(text) = processed {
        db.query(
            "UPDATE transcription_history SET post_processed_text=? WHERE id=?",
            &[text.into(), id.to_string()],
        )
        .unwrap();
    }
}

#[test]
fn history_search_pages_before_loading_full_transcripts() {
    let (_dir, _paths, db) = fixture();
    for i in 1..=105 {
        insert(
            &db,
            i,
            &format!("transcript {i} ' % _ Indonesian 日本語"),
            None,
        );
    }
    let rows = db.history("Indonesian 日本語", false, 0, 40).unwrap();
    assert_eq!(rows.len(), 41);
    assert_eq!(rows[0]["id"], 105);
    assert_eq!(db.history("Indonesian", false, 1, 40).unwrap()[0]["id"], 65);
    assert_eq!(db.history("Indonesian", false, 2, 40).unwrap().len(), 25);
    assert!(db.history("' OR 1=1 --", false, 0, 40).unwrap().is_empty());
    assert_eq!(db.history("% _", false, 0, 40).unwrap().len(), 41);
    assert_eq!(db.entry(None).unwrap()["id"], 105);
}

#[test]
fn processed_text_keeps_empty_value_and_falls_back_only_for_null() {
    let (_dir, paths, db) = fixture();
    insert(&db, 1, "raw quotation: \" $() {query} 日本語", None);
    insert(&db, 2, "raw", Some(""));
    assert_eq!(
        execute(&paths, &json!({"op":"copy","id":2})).unwrap()["alfredworkflow"]["arg"],
        ""
    );
    assert_eq!(
        execute(&paths, &json!({"op":"raw","id":2})).unwrap()["alfredworkflow"]["arg"],
        "raw"
    );
    assert_eq!(
        execute(&paths, &json!({"op":"copy","id":1})).unwrap()["alfredworkflow"]["arg"],
        "raw quotation: \" $() {query} 日本語"
    );
    assert!(execute(&paths, &json!({"op":"copy","id":"1 OR 1=1"})).is_err());
}

#[test]
fn saved_flag_and_delete_preserve_a_recoverable_backup() {
    let (_dir, paths, db) = fixture();
    insert(&db, 1, "valuable transcript", Some("processed"));
    fs::write(paths.data.join("recordings/1.wav"), b"audio fixture").unwrap();
    execute(&paths, &json!({"op":"save","id":1})).unwrap();
    assert_eq!(db.history("", true, 0, 40).unwrap().len(), 1);
    execute(&paths, &json!({"op":"delete","id":1})).unwrap();
    assert!(db.entry(Some(1)).is_err());
    assert!(!paths.data.join("recordings/1.wav").exists());
    let backup = fs::read_dir(&paths.trash)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let record: Value =
        serde_json::from_slice(&fs::read(backup.join("transcript.json")).unwrap()).unwrap();
    assert_eq!(record["transcription_text"], "valuable transcript");
    assert_eq!(
        fs::read(backup.join("audio-1.wav")).unwrap(),
        b"audio fixture"
    );
}

#[test]
fn deletion_rejects_traversal_symlinks_and_backup_failures() {
    let (dir, paths, db) = fixture();
    insert(&db, 1, "keep me", None);
    let outside = dir.path().join("outside.wav");
    fs::write(&outside, b"untouched").unwrap();
    db.query(
        "UPDATE transcription_history SET file_name='../outside.wav' WHERE id=1",
        &[],
    )
    .unwrap();
    assert!(delete(&paths, 1).is_err());
    db.query(
        "UPDATE transcription_history SET file_name='1.wav' WHERE id=1",
        &[],
    )
    .unwrap();
    symlink(&outside, paths.data.join("recordings/1.wav")).unwrap();
    assert!(delete(&paths, 1).is_err());
    fs::remove_file(paths.data.join("recordings/1.wav")).unwrap();
    fs::write(&paths.trash, b"not a folder").unwrap();
    assert!(delete(&paths, 1).is_err());
    assert_eq!(db.entry(Some(1)).unwrap()["transcription_text"], "keep me");
    assert_eq!(fs::read(outside).unwrap(), b"untouched");
}

#[test]
fn settings_preserve_unknown_keys_and_dictionary_is_case_insensitive() {
    let (dir, _, _) = fixture();
    let path = dir.path().join("settings.json");
    let mut store = json!({"other":{"keep":true},"settings":{"post_process_api_keys":{"provider":"fixture-secret"},"selected_model":"small","selected_language":"en","custom_words":["Rust"]}});
    assert!(!app::mutate(&mut store, &json!({"op":"word-add","value":" rust "})).unwrap());
    app::mutate(&mut store, &json!({"op":"word-add","value":" 日本語 "})).unwrap();
    assert!(app::mutate(&mut store, &json!({"op":"word-add","value":"a\nb"})).is_err());
    app::mutate(&mut store, &json!({"op":"model","value":"large"})).unwrap();
    assert_eq!(store["settings"]["selected_language"], "auto");
    app::atomic_store(&path, &store).unwrap();
    let read: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(read["other"]["keep"], true);
    assert_eq!(
        read["settings"]["post_process_api_keys"]["provider"],
        "fixture-secret"
    );
    assert_eq!(read["settings"]["custom_words"], json!(["Rust", "日本語"]));
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn model_discovery_handles_hf_symlinks_partial_downloads_and_legacy_ids() {
    let (_dir, paths, _) = fixture();
    let repo = paths.hub.join("models--handy-computer--whisper-small-gguf");
    fs::create_dir_all(repo.join("snapshots/ready")).unwrap();
    fs::create_dir_all(repo.join("snapshots/incomplete")).unwrap();
    fs::create_dir_all(repo.join("refs")).unwrap();
    fs::create_dir_all(repo.join("blobs")).unwrap();
    fs::write(repo.join("refs/main"), "incomplete\n").unwrap();
    fs::write(repo.join("blobs/model"), b"model fixture").unwrap();
    symlink(
        "../../blobs/model",
        repo.join("snapshots/ready/whisper-small-Q8_0.gguf"),
    )
    .unwrap();
    symlink(
        "../../blobs/missing",
        repo.join("snapshots/incomplete/missing.gguf"),
    )
    .unwrap();
    fs::create_dir_all(paths.data.join("models")).unwrap();
    fs::write(paths.data.join("models/ggml-small.bin"), b"legacy").unwrap();
    fs::write(
        paths.data.join("models/incomplete.gguf.partial"),
        b"partial",
    )
    .unwrap();
    let models = models::downloaded(&paths.hub, &paths.data.join("models")).unwrap();
    assert_eq!(models.len(), 2);
    assert!(models
        .iter()
        .any(|m| text(m, "id") == "handy-computer/whisper-small-gguf/whisper-small-Q8_0.gguf"));
    assert!(models.iter().any(|m| text(m, "id") == "small"));
    assert!(models::languages("small")
        .iter()
        .any(|l| text(l, "code") == "id"));
    assert!(models::languages("parakeet-tdt-0.6b-v2").is_empty());
    assert!(models::languages("canary-180m-flash")
        .iter()
        .all(|l| ["auto", "en", "de", "es", "fr"].contains(&text(l, "code"))));
}

#[test]
fn missing_or_malformed_data_is_not_created_or_overwritten() {
    let (_dir, paths, db) = fixture();
    drop(db);
    fs::remove_file(paths.db()).unwrap();
    assert_eq!(
        filter(&paths, "history ").unwrap()["items"][0]["valid"],
        false
    );
    assert!(!paths.db().exists());
    let config = paths.data.join("settings_store.json");
    fs::write(&config, "{broken").unwrap();
    assert!(paths
        .apply(&json!({"op":"word-add","value":"test"}))
        .is_err());
    assert_eq!(fs::read_to_string(config).unwrap(), "{broken");
    assert!(execute(&paths, &json!({"op":"copy"})).is_err());
}
