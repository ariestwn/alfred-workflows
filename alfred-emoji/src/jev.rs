use crate::catalog::Catalog;
use crate::Result;
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    thread,
    time::{Duration, Instant},
};

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const BEAM: usize = 3;
const MAX_OPTIONS: usize = 200;
const NONE: &str = "none";

pub struct Jev {
    pub key: String,
    pub model: String,
}

/// A subgroup small enough to be one Choice question.
pub struct Leaf {
    pub label: String,
    pub members: Vec<usize>,
}

pub fn leaves(catalog: &Catalog) -> Vec<Leaf> {
    let mut order: Vec<(&str, &str)> = Vec::new();
    let mut members: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, e) in catalog.emoji.iter().enumerate() {
        members.entry(e.subgroup).or_insert_with(|| {
            order.push((e.group, e.subgroup));
            Vec::new()
        }).push(i);
    }
    let mut out = Vec::new();
    for (group, subgroup) in order {
        let all = &members[subgroup];
        let parts = all.len().div_ceil(MAX_OPTIONS);
        let size = all.len().div_ceil(parts);
        for (n, chunk) in all.chunks(size).enumerate() {
            let label = if parts == 1 {
                format!("{group} › {subgroup}")
            } else {
                format!("{group} › {subgroup} (part {})", n + 1)
            };
            out.push(Leaf { label, members: chunk.to_vec() });
        }
    }
    out
}

/// Two speculative readings of the same query, asked in parallel.
pub const READINGS: [(&str, &str); 2] = [
    ("literal", "Read the query literally: the object, action, or feeling it names or describes."),
    ("slang", "Read the query the way people actually use emoji in chat: slang, innuendo, sexual or flirty meaning, memes, and internet culture. For example, a crude or sexual phrase maps to emoji people send for sex and flirting, not only to anger."),
];

pub fn subgroup_question(catalog: &Catalog, leaves: &[Leaf], reading: &str) -> Value {
    let mut criteria = Map::new();
    for leaf in leaves {
        let names: Vec<&str> = leaf.members.iter().take(10).map(|&i| catalog.emoji[i].name).collect();
        criteria.insert(leaf.label.clone(), json!({ "examples": names }));
    }
    json!({
        "type": "choice",
        "instructions": {
            "task": "A person typed `query` into an emoji picker. The query may be an emoji name, a keyword, a feeling, a situation, or a phrase they want to react to. Which emoji category most likely contains an emoji they want?",
            "reading": reading,
        },
        "criteria": criteria,
    })
}

pub fn emoji_question(catalog: &Catalog, leaf: &Leaf) -> Value {
    let mut criteria = Map::new();
    for &i in &leaf.members {
        let e = &catalog.emoji[i];
        let value = if e.keywords.is_empty() { Value::Null } else { json!(e.keywords.join(", ")) };
        criteria.insert(e.name.to_string(), value);
    }
    criteria.insert(NONE.into(), json!("No emoji listed here fits what the person wants to express."));
    json!({
        "type": "choice",
        "instructions": format!("A person typed `query` into an emoji picker. Consider both its literal meaning and how people use emoji in chat, including slang, innuendo, and sexual or flirty meaning. Among these emoji from the category \"{}\", which one would they most likely pick?", leaf.label),
        "criteria": criteria,
    })
}

fn probabilities(answers: &Value, id: &str) -> Result<Vec<(String, f64)>> {
    let map = answers[id]["probabilities"]
        .as_object()
        .ok_or_else(|| format!("Jev response is missing answer `{id}`."))?;
    let mut out: Vec<(String, f64)> =
        map.iter().map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(0.0))).collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    Ok(out)
}

/// Combines each beam's subgroup and emoji probabilities into a
/// geometric-mean path score, ranked high to low.
pub fn rank(
    catalog: &Catalog,
    beams: &[(&Leaf, f64)],
    picks: &[Vec<(String, f64)>],
    limit: usize,
    min_score: f64,
) -> Vec<(usize, f64)> {
    let mut scored: Vec<(usize, f64)> = Vec::new();
    for ((leaf, p_leaf), dist) in beams.iter().zip(picks) {
        for (name, p) in dist {
            if name == NONE {
                continue;
            }
            let Some(&i) = leaf.members.iter().find(|&&i| catalog.emoji[i].name == name) else {
                continue;
            };
            let score = (p_leaf * p).sqrt();
            if score >= min_score {
                scored.push((i, score));
            }
        }
    }
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.dedup_by_key(|s| s.0);
    scored.truncate(limit);
    scored
}

impl Jev {
    fn ask(&self, query: &str, questions: Value) -> Result<Value> {
        let body = json!({ "state": { "query": query }, "model": self.model, "questions": questions });
        let mut delay = Duration::from_millis(400);
        for attempt in 0..3 {
            let started = Instant::now();
            let response = ureq::post(ENDPOINT)
                .timeout(Duration::from_secs(30))
                .set("Authorization", &format!("Bearer {}", self.key))
                .send_json(&body);
            if std::env::var_os("EMOJI_DEBUG").is_some() {
                let size = body.to_string().len();
                eprintln!("jev: {size} byte request, {:?}, ok={}", started.elapsed(), response.is_ok());
            }
            match response {
                Ok(r) => {
                    let v: Value = r.into_json().map_err(|e| format!("Jev returned invalid JSON: {e}"))?;
                    return Ok(v["answers"].clone());
                }
                Err(ureq::Error::Status(429 | 529, _)) if attempt < 2 => {
                    thread::sleep(delay);
                    delay *= 2;
                }
                Err(ureq::Error::Status(401, _)) => return Err("TypeSafe API key was rejected.".into()),
                Err(ureq::Error::Status(code, r)) => {
                    return Err(format!("Jev error {code}: {}", r.into_string().unwrap_or_default()))
                }
                Err(e) => return Err(format!("Cannot reach TypeSafe: {e}")),
            }
        }
        Err("TypeSafe is busy. Try again shortly.".into())
    }

    pub fn search(&self, catalog: &Catalog, query: &str, limit: usize) -> Result<Vec<(usize, f64)>> {
        let leaves = leaves(catalog);
        let mut questions = Map::new();
        for (id, reading) in READINGS {
            questions.insert(id.into(), subgroup_question(catalog, &leaves, reading));
        }
        let first = self.ask(query, Value::Object(questions))?;
        let mut beams: Vec<(&Leaf, f64)> = Vec::new();
        for (id, _) in READINGS {
            for (label, p) in probabilities(&first, id)?.into_iter().take(BEAM) {
                let Some(leaf) = leaves.iter().find(|l| l.label == label) else { continue };
                match beams.iter_mut().find(|b| std::ptr::eq(b.0, leaf)) {
                    Some(b) => b.1 = b.1.max(p),
                    None => beams.push((leaf, p)),
                }
            }
        }
        let mut questions = Map::new();
        for (n, (leaf, _)) in beams.iter().enumerate() {
            questions.insert(format!("pick_{n}"), emoji_question(catalog, leaf));
        }
        let second = self.ask(query, Value::Object(questions))?;
        let picks = (0..beams.len())
            .map(|n| probabilities(&second, &format!("pick_{n}")))
            .collect::<Result<Vec<_>>>()?;
        Ok(rank(catalog, &beams, &picks, limit, 0.05))
    }
}
