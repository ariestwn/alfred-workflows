use crate::model::Event;
use regex::Regex;
use std::sync::OnceLock;

pub fn host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.contains('@') || authority.contains('\\') || url.chars().any(char::is_control) {
        return None;
    }
    let host = authority.strip_suffix(":443").unwrap_or(authority);
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
    {
        return None;
    }
    Some(host.to_ascii_lowercase())
}
pub fn provider(url: &str) -> Option<&'static str> {
    let host = host(url)?;
    for (domain, label) in [
        ("zoom.us", "Zoom"),
        ("zoom.com", "Zoom"),
        ("meet.google.com", "Google Meet"),
        ("teams.microsoft.com", "Microsoft Teams"),
        ("teams.live.com", "Microsoft Teams"),
        ("teams.cloud.microsoft", "Microsoft Teams"),
        ("slack.com", "Slack Huddle"),
        ("webex.com", "Webex"),
        ("facetime.apple.com", "FaceTime"),
        ("skype.com", "Skype"),
        ("bluejeans.com", "BlueJeans"),
        ("chime.aws", "Amazon Chime"),
        ("whereby.com", "Whereby"),
        ("meet.jit.si", "Jitsi"),
        ("jitsi.org", "Jitsi"),
        ("around.co", "Around"),
        ("chorus.ai", "Chorus"),
        ("riverside.fm", "Riverside"),
        ("streamyard.com", "StreamYard"),
    ] {
        if host == domain || host.ends_with(&format!(".{domain}")) {
            return Some(label);
        }
    }
    None
}
pub fn conference(event: &Event) -> Option<(String, &'static str)> {
    static URLS: OnceLock<Regex> = OnceLock::new();
    let regex = URLS.get_or_init(|| Regex::new(r#"https://[^\s<>\"']+"#).unwrap());
    for text in [&event.url, &event.location, &event.notes] {
        for candidate in regex.find_iter(text) {
            let url = candidate
                .as_str()
                .trim_end_matches(['.', ',', ';', ')', ']', '}'])
                .replace("&amp;", "&");
            if let Some(provider) = provider(&url) {
                return Some((url, provider));
            }
        }
    }
    None
}
