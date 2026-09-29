use super::*;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

fn config(values: &[(&str, &str)]) -> Config {
    Config::load(|key| {
        values
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_string())
            .unwrap_or_default()
    })
    .unwrap()
}

#[test]
fn provider_switch_uses_independent_settings() {
    for (provider, model, effort) in [
        ("claude", "sonnet", "high"),
        ("codex", "some-future-model", "low"),
        ("opencode", "deepseek/deepseek-flash", ""),
    ] {
        let c = config(&[
            ("QUICKAI_PROVIDER", provider),
            ("QUICKAI_CLAUDE_MODEL", "sonnet"),
            ("QUICKAI_CLAUDE_EFFORT", "high"),
            ("QUICKAI_CODEX_MODEL", "some-future-model"),
            ("QUICKAI_CODEX_EFFORT", "low"),
            ("QUICKAI_OPENCODE_MODEL", "deepseek/deepseek-flash"),
            ("QUICKAI_OPENCODE_EFFORT", ""),
        ]);
        assert_eq!((c.model.as_str(), c.effort.as_str()), (model, effort));
    }
}

#[test]
fn opencode_rejects_unexpected_effort() {
    assert!(Config::load(|key| match key {
        "QUICKAI_PROVIDER" => "opencode".into(),
        "QUICKAI_OPENCODE_EFFORT" => "high".into(),
        _ => String::new(),
    })
    .is_err());
}

#[test]
fn invalid_settings_fail_before_calling_provider() {
    for (key, value) in [
        ("QUICKAI_PROVIDER", "unknown"),
        ("QUICKAI_TIMEOUT", "0"),
        ("QUICKAI_TIMEOUT", "601"),
        ("QUICKAI_TIMEOUT", "abc"),
        ("QUICKAI_CODEX_EFFORT", "high\"; bad"),
        ("QUICKAI_OUTPUT", "unknown"),
    ] {
        assert!(Config::load(|k| if k == key {
            value.into()
        } else {
            String::new()
        })
        .is_err());
    }
}

#[test]
fn input_limits_count_unicode_characters() {
    assert!(prompt::validate_input(&"é".repeat(16_000)).is_ok());
    assert!(prompt::validate_input(&"🦀".repeat(16_001)).is_err());
    assert!(prompt::validate_input(" \n\t").is_err());
    assert!(prompt::validate_input("hello\0world").is_err());
}

#[test]
fn cleanup_preserves_user_formatting_and_code() {
    assert_eq!(
        prompt::clean_output("```text\nBetter text.\n```", "  original\n").unwrap(),
        "  Better text.\n"
    );
    let code = "```rust\nfn main() {}\n```";
    assert_eq!(prompt::clean_output(code, code).unwrap(), code);
    assert_eq!(
        prompt::clean_output("- One\n  - Two", "- 1\n  - 2").unwrap(),
        "- One\n  - Two"
    );
    assert!(prompt::clean_output("```\n\n```", "hi").is_err());
    assert!(prompt::clean_output(" \n", "hi").is_err());
}

#[test]
fn claude_errors_never_become_rewritten_text() {
    assert_eq!(
        runner::claude_result(
            r#"{"type":"result","subtype":"success","is_error":false,"result":"It's fixed."}"#
        )
        .unwrap(),
        "It's fixed."
    );
    for raw in [
        r#"{"is_error":true,"result":"Not logged in"}"#,
        r#"{"subtype":"error_max_turns","errors":["limit"]}"#,
        "not json",
        r#"{"type":"result"}"#,
    ] {
        assert!(runner::claude_result(raw).is_err());
    }
}

#[test]
fn failed_workflow_envelope_has_no_pastable_text() {
    let value = envelope(None, "copy", "Login failed");
    assert_eq!(value["alfredworkflow"]["arg"], "");
    assert_eq!(value["alfredworkflow"]["variables"]["quickai_ok"], "0");
}

#[test]
fn paste_requires_same_application_and_selection() {
    let old = Some(selection::Snapshot {
        pid: 1,
        text: Some("hello".into()),
    });
    assert!(selection::paste_target_unchanged(&old, &old));
    assert!(!selection::paste_target_unchanged(&old, &None));
    assert!(!selection::paste_target_unchanged(&None, &None));
    let unavailable = Some(selection::Snapshot { pid: 1, text: None });
    assert!(!selection::paste_target_unchanged(
        &unavailable,
        &unavailable
    ));
    assert!(!selection::paste_target_unchanged(
        &old,
        &Some(selection::Snapshot {
            pid: 2,
            text: Some("hello".into())
        })
    ));
    assert!(!selection::paste_target_unchanged(
        &old,
        &Some(selection::Snapshot {
            pid: 1,
            text: Some("changed".into())
        })
    ));
}

fn mock(script: &str) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mock cli");
    fs::write(&path, format!("#!/usr/bin/python3\n{script}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    (directory, path)
}

#[test]
fn codex_reads_only_final_message_and_passes_options_literally() {
    let (dir, path) = mock(
        r#"
import sys, pathlib
a = sys.argv[1:]
assert a[0] == 'exec' and a[-1] == '-'
assert a[a.index('--model') + 1] == 'model with spaces; $(echo wrong)'
assert 'model_reasoning_effort="low"' in a
assert '--ignore-user-config' in a and '--ephemeral' in a
text = sys.stdin.read()
assert 'aku' in text and '$(touch' in text
print('THIS IS PROGRESS, NOT THE RESULT')
print('x' * 150000, file=sys.stderr)
pathlib.Path(a[a.index('--output-last-message') + 1]).write_text('Aku sudah memperbaikinya.\n')
"#,
    );
    let mut c = config(&[
        ("QUICKAI_CODEX_MODEL", "model with spaces; $(echo wrong)"),
        ("QUICKAI_CODEX_EFFORT", "low"),
    ]);
    c.executable = path;
    let input = format!("aku $(touch {})", dir.path().join("injected").display());
    let output = runner::transform(&c, "Edit only", &input).unwrap();
    assert_eq!(output, "Aku sudah memperbaikinya.");
    assert!(!dir.path().join("injected").exists());
}

#[test]
fn claude_receives_model_effort_and_disabled_tools() {
    let (_dir, path) = mock(
        r#"
import sys, json
a = sys.argv[1:]
assert a[a.index('--model')+1] == 'sonnet'
assert a[a.index('--effort')+1] == 'medium'
assert a[a.index('--tools')+1] == ''
assert '--no-session-persistence' in a
assert json.loads(a[a.index('--mcp-config')+1]) == {'mcpServers':{}}
assert 'original' in sys.stdin.read()
print(json.dumps({'type':'result','subtype':'success','is_error':False,'result':'Rewritten.'}))
"#,
    );
    let mut c = config(&[
        ("QUICKAI_PROVIDER", "claude"),
        ("QUICKAI_CLAUDE_MODEL", "sonnet"),
        ("QUICKAI_CLAUDE_EFFORT", "medium"),
    ]);
    c.executable = path;
    assert_eq!(
        runner::transform(&c, "Edit only", "original").unwrap(),
        "Rewritten."
    );
}

#[test]
fn opencode_writes_private_config_parses_final_text_and_discards_session() {
    let project = tempfile::tempdir().unwrap();
    let log = project.path().join("sessions.txt");
    let script = format!(
        r#"
import sys, json, pathlib
a = sys.argv[1:]
if a and a[0] == 'session':
    pathlib.Path({log:?}).write_text(json.dumps(a))
    sys.exit(0)
assert a[0] == 'run'
assert a[a.index('--format') + 1] == 'json'
agent = a[a.index('--agent') + 1]
assert agent == 'quickai'
assert a[a.index('--model') + 1] == 'deepseek/deepseek-flash'
assert 'original' in sys.stdin.read()
cfg = json.loads(pathlib.Path('opencode.json').read_text())
deny = [{{'action': '*', 'resource': '*', 'effect': 'deny'}}]
assert cfg['permissions'] == deny
assert cfg['agents'][agent]['permissions'] == deny
assert 'Edit only' in cfg['agents'][agent]['system']
print(json.dumps({{'type': 'step_start', 'sessionID': 'ses_mock', 'part': {{'type': 'step-start'}}}}))
print(json.dumps({{'type': 'text', 'sessionID': 'ses_mock', 'part': {{'type': 'reasoning', 'text': 'PRIVATE'}}}}))
print(json.dumps({{'type': 'text', 'sessionID': 'ses_mock', 'part': {{'type': 'text', 'text': 'Rewritten.'}}}}))
"#,
        log = log.to_str().unwrap()
    );
    let (_mock_dir, path) = mock(&script);
    let mut c = config(&[
        ("QUICKAI_PROVIDER", "opencode"),
        ("QUICKAI_OPENCODE_MODEL", "deepseek/deepseek-flash"),
        ("QUICKAI_OPENCODE_DIR", project.path().to_str().unwrap()),
    ]);
    c.executable = path;
    assert_eq!(
        runner::transform(&c, "Edit only", "original").unwrap(),
        "Rewritten."
    );
    let logged: Vec<String> = serde_json::from_str(&fs::read_to_string(&log).unwrap()).unwrap();
    assert_eq!(logged, ["session", "delete", "ses_mock"]);
}

#[test]
fn opencode_reasoning_and_errors_never_become_text() {
    let parsed = runner::opencode_parse(concat!(
        "{\"type\":\"step_start\",\"sessionID\":\"ses_1\",\"part\":{\"type\":\"step-start\"}}\n",
        "{\"type\":\"text\",\"sessionID\":\"ses_1\",\"part\":{\"type\":\"text\",\"text\":\"Draft\"}}\n",
        "{\"type\":\"step_start\",\"sessionID\":\"ses_1\",\"part\":{\"type\":\"step-start\"}}\n",
        "{\"type\":\"text\",\"sessionID\":\"ses_1\",\"part\":{\"type\":\"reasoning\",\"text\":\"PRIVATE\"}}\n",
        "{\"type\":\"text\",\"sessionID\":\"ses_1\",\"part\":{\"type\":\"text\",\"text\":\"Final\"}}\n",
        "not json\n",
    ));
    assert_eq!((parsed.text.as_str(), parsed.session.as_str()), ("Final", "ses_1"));
    assert!(parsed.error.is_empty());
    let failure = runner::opencode_parse(
        "{\"type\":\"error\",\"sessionID\":\"ses_2\",\"error\":{\"message\":\"Model unavailable\"}}\n",
    );
    assert_eq!((failure.error.as_str(), failure.session.as_str()), ("Model unavailable", "ses_2"));
    assert!(failure.text.is_empty());
}

#[test]
fn opencode_nonzero_exit_exposes_event_error() {
    let (_dir, path) = mock(
        r#"
import sys, json
if sys.argv[1:2] == ['session']:
    sys.exit(0)
print(json.dumps({'type': 'error', 'sessionID': 'ses_bad', 'error': {'message': 'Model unavailable'}}))
sys.exit(1)
"#,
    );
    let project = tempfile::tempdir().unwrap();
    let mut c = config(&[
        ("QUICKAI_PROVIDER", "opencode"),
        ("QUICKAI_OPENCODE_DIR", project.path().to_str().unwrap()),
    ]);
    c.executable = path;
    assert_eq!(
        runner::transform(&c, "Edit", "original").unwrap_err(),
        "OpenCode: Model unavailable"
    );
}

#[test]
fn nonzero_exit_missing_answer_and_empty_answer_fail() {
    for script in ["import sys; print('Login required', file=sys.stderr); sys.exit(2)",
        "print('no final file')", "import sys,pathlib; a=sys.argv; pathlib.Path(a[a.index('--output-last-message')+1]).write_text('')"] {
        let (_dir, path) = mock(script);
        let mut c = config(&[]); c.executable = path;
        assert!(runner::transform(&c, "Edit", "original").is_err());
    }
}

#[test]
fn claude_nonzero_exit_exposes_structured_error_message() {
    let (_dir, path) = mock("import sys,json; print(json.dumps({'is_error':True,'usage':{'metadata':'x'*900},'result':'Please log in again.'})); sys.exit(1)");
    let mut c = config(&[("QUICKAI_PROVIDER", "claude")]);
    c.executable = path;
    assert_eq!(
        runner::transform(&c, "Edit", "original").unwrap_err(),
        "Claude: Please log in again."
    );
}

#[test]
fn timeout_kills_descendants_even_when_stdin_is_not_read() {
    let marker = tempfile::tempdir().unwrap();
    let marker_path = marker.path().join("orphan");
    let child_code = format!(
        "import time,pathlib; time.sleep(0.6); pathlib.Path({:?}).touch()",
        marker_path.to_str().unwrap()
    );
    let script = format!("import subprocess,time\nsubprocess.Popen(['/usr/bin/python3','-c',{child_code:?}])\ntime.sleep(30)");
    let (_dir, path) = mock(&script);
    let mut c = config(&[]);
    c.executable = path;
    c.timeout = Duration::from_millis(200);
    assert!(runner::transform(&c, "Edit", &"x".repeat(16000))
        .unwrap_err()
        .contains("timed out"));
    std::thread::sleep(Duration::from_millis(700));
    assert!(!marker_path.exists());
}

#[test]
fn clipboard_verification_requires_fresh_matching_selection_in_same_app() {
    assert!(clipboard::matches_original(
        "hello",
        42,
        &Ok(("hello".into(), 42))
    ));
    assert!(!clipboard::matches_original(
        "hello",
        42,
        &Ok(("hello".into(), 43))
    ));
    assert!(!clipboard::matches_original(
        "hello",
        42,
        &Ok(("changed".into(), 42))
    ));
    assert!(!clipboard::matches_original(
        "hello",
        42,
        &Err("No new clipboard contents".into())
    ));
}
