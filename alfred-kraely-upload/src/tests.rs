use super::*;
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    time::{Duration, Instant},
};

fn config(values: &[(&str, &str)]) -> Result<Config> {
    Config::from_lookup(|name| {
        values
            .iter()
            .chain(&[("KRAELY_HOST", "my-server")])
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.to_string())
    })
}

fn fixture(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

fn executable(dir: &Path, contents: &str) -> PathBuf {
    let path = fixture(dir, "mock-scp", contents);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[test]
fn configuration_defaults_and_overrides() {
    let c = config(&[]).unwrap();
    assert_eq!(c.host, "my-server");
    assert_eq!(c.directory, "/home/ubuntu/screenshot/");
    assert_eq!(c.max_bytes, 20 * 1024 * 1024);
    assert_eq!(c.timeout, Duration::from_secs(60));
    let c = config(&[
        ("KRAELY_HOST", "ubuntu@my-vps"),
        ("KRAELY_REMOTE_DIR", "/srv/my files/"),
        ("KRAELY_MAX_SIZE_MB", "0.5"),
        ("KRAELY_TIMEOUT", "180"),
    ])
    .unwrap();
    assert_eq!(c.max_bytes, 512 * 1024);
    assert_eq!(c.timeout, Duration::from_secs(180));
    assert!(config(&[("KRAELY_HOST", "ubuntu@[2001:db8::1]")]).is_ok());
}

#[test]
fn host_is_required_and_trimmed() {
    assert!(Config::from_lookup(|_| None)
        .unwrap_err()
        .contains("Set an SSH host"));
    assert!(config(&[("KRAELY_HOST", "  ")])
        .unwrap_err()
        .contains("Set an SSH host"));
    let c = config(&[("KRAELY_HOST", " ubuntu@new-vps ")]).unwrap();
    assert_eq!(c.host, "ubuntu@new-vps");
}

#[test]
fn rejects_invalid_settings_before_a_process_can_start() {
    for host in [
        "-oProxyCommand=echo",
        "host:/other",
        "$(touch sentinel)",
        "a b",
        "a\nb",
        "user@@host",
        "-user@host",
        "user@-host",
    ] {
        assert!(config(&[("KRAELY_HOST", host)]).is_err(), "{host}");
    }
    for dir in ["~/uploads", "relative", "/a\nb", "/a\0b"] {
        assert!(config(&[("KRAELY_REMOTE_DIR", dir)]).is_err());
    }
    for size in ["0", "-1", "NaN", "inf", "words"] {
        assert!(config(&[("KRAELY_MAX_SIZE_MB", size)]).is_err());
    }
    for timeout in ["0", "601", "-1", "words"] {
        assert!(config(&[("KRAELY_TIMEOUT", timeout)]).is_err());
    }
}

#[test]
fn rejects_missing_files_directories_and_oversize_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = config(&[]).unwrap();
    c.max_bytes = 4;
    assert!(upload::Upload::prepare(&c, &dir.path().join("missing")).is_err());
    assert!(upload::Upload::prepare(&c, dir.path())
        .unwrap_err()
        .contains("single file"));
    let path = fixture(dir.path(), "file", "12345");
    assert!(upload::Upload::prepare(&c, &path)
        .unwrap_err()
        .contains("too large"));
    fs::write(&path, "1234").unwrap();
    assert!(upload::Upload::prepare(&c, &path).is_ok());
    let bad = fixture(dir.path(), "line\nbreak", "x");
    assert!(upload::Upload::prepare(&c, &bad).is_err());
}

#[test]
fn paths_are_literal_arguments_and_preserve_spaces_unicode_and_metacharacters() {
    let dir = tempfile::tempdir().unwrap();
    let name = "-report: it's {query} $(touch owned) `id` 🦀.png";
    let local = fixture(dir.path(), name, "sample");
    let c = config(&[("KRAELY_REMOTE_DIR", "/srv/team's $(stuff)/")]).unwrap();
    let plan = upload::Upload::prepare(&c, &local).unwrap();
    assert_eq!(plan.remote, format!("/srv/team's $(stuff)/{name}"));
    let cmd = upload::command(&c, &plan, Path::new("/usr/bin/scp"));
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    assert_eq!(
        args[args.len() - 2],
        std::path::absolute(local).unwrap().to_str().unwrap()
    );
    assert_eq!(args.last().unwrap(), "my-server:/srv/team's $(stuff)/");
    assert!(args.iter().any(|s| s == "BatchMode=yes"));
    assert!(args.iter().any(|s| s == "StrictHostKeyChecking=yes"));
    assert!(!args.iter().any(|s| s == "-O"));
    // A file symlink uploads its content using the selected name.
    let link = dir.path().join("friendly name.png");
    symlink(&plan.local, &link).unwrap();
    assert!(upload::Upload::prepare(&c, &link)
        .unwrap()
        .remote
        .ends_with("/friendly name.png"));
}

#[test]
fn successful_transfer_returns_exact_path_and_failure_never_copies() {
    let dir = tempfile::tempdir().unwrap();
    let local = fixture(dir.path(), "report.pdf", "sample");
    let c = config(&[]).unwrap();
    let plan = upload::Upload::prepare(&c, &local).unwrap();
    let scp = executable(dir.path(), "#!/bin/sh\nexit 0\n");
    upload::execute_with(&c, &plan, &scp).unwrap();
    let output = envelope(&Ok(plan.remote.clone()), None);
    assert_eq!(
        output["alfredworkflow"]["arg"],
        "/home/ubuntu/screenshot/report.pdf"
    );
    assert_eq!(output["alfredworkflow"]["variables"]["kraely_ok"], "1");
    fs::write(
        &scp,
        "#!/bin/sh\nprintf 'Permission denied (publickey).\\n' >&2\nexit 255\n",
    )
    .unwrap();
    let error = upload::execute_with(&c, &plan, &scp).unwrap_err();
    assert!(error.contains("authentication failed"));
    let failed = envelope(&Err(error), None);
    assert_eq!(failed["alfredworkflow"]["arg"], "");
    assert_eq!(failed["alfredworkflow"]["variables"]["kraely_ok"], "0");
}

#[test]
fn timeout_kills_scp_and_its_children() {
    let dir = tempfile::tempdir().unwrap();
    let local = fixture(dir.path(), "source", "sample");
    let mut c = config(&[]).unwrap();
    c.timeout = Duration::from_millis(150);
    let plan = upload::Upload::prepare(&c, &local).unwrap();
    // The child would write a sentinel after timeout if only the parent were killed.
    let scp = executable(dir.path(), "#!/bin/sh\nfor arg do last=$arg; done\n(sleep 0.6; printf leaked > \"$0.leaked\") &\nwait\n");
    let started = Instant::now();
    assert!(upload::execute_with(&c, &plan, &scp)
        .unwrap_err()
        .contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(2));
    std::thread::sleep(Duration::from_millis(650));
    assert!(!dir.path().join("mock-scp.leaked").exists());
}
