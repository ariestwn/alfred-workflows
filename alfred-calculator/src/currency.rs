use crate::{
    config::Config,
    info,
    math::Math,
    rates::{self, Rates},
    Result,
};
use serde_json::{json, Value};

fn matches(query: &str, code: &str, name: &str) -> bool {
    let q = query.to_lowercase();
    code.to_lowercase().starts_with(&q)
        || q.split_whitespace()
            .all(|token| name.to_lowercase().contains(token))
}
fn exact(query: &str) -> Option<String> {
    let q = query.trim().to_lowercase();
    if let Some((code, _)) = rates::names()
        .iter()
        .find(|(code, name)| code.to_lowercase() == q || name.to_lowercase() == q)
    {
        return Some(code.clone());
    }
    match q.as_str() {
        "$" | "us dollar" | "us dollars" => Some("USD".into()),
        "€" | "euro" | "euros" => Some("EUR".into()),
        "£" | "british pound" => Some("GBP".into()),
        "rp" | "rupiah" => Some("IDR".into()),
        "yen" | "¥" => Some("JPY".into()),
        "bitcoin" => Some("BTC".into()),
        "ethereum" => Some("ETH".into()),
        _ => None,
    }
}
fn ordered(config: &Config) -> Vec<(&'static String, &'static String)> {
    let mut names: Vec<_> = rates::names().iter().collect();
    names.sort_by_key(|(code, _)| {
        (
            config
                .favorites
                .iter()
                .position(|c| c == *code)
                .unwrap_or(usize::MAX),
            *code,
        )
    });
    names
}
pub fn browse(query: &str, config: &Config, math: &Math, rates: &Rates) -> Result<Vec<Value>> {
    let Some(c) = re!(r"^\s*([+-]?(?:\d[\d.,]*|[.,]\d+)(?:[kKMB])?)\s*(.*)$").captures(query)
    else {
        return Ok(vec![info(
            "Enter an amount, then a currency",
            "Examples: 100 · 100 usd · 100 usd to rupiah",
        )]);
    };
    let amount = &c[1];
    let remaining = c[2].trim();
    let mut context = math.context(config.ppi)?;
    let normalized_amount = math.normalize(amount);
    let amount_answer = math.eval_context(&normalized_amount, &mut context)?;
    let connector = re!(r"(?i)\s+(?:to|in|as)(?:\s+|$)").find(remaining);
    let (source, target) = if let Some(connector) = connector {
        (
            remaining[..connector.start()].trim(),
            remaining[connector.end()..].trim(),
        )
    } else {
        let mut split = None;
        for (index, _) in remaining
            .char_indices()
            .filter(|(_, c)| c.is_whitespace())
            .rev()
        {
            if exact(&remaining[..index]).is_some() {
                split = Some((&remaining[..index], remaining[index..].trim()));
                break;
            }
        }
        split.unwrap_or((remaining, ""))
    };
    let source_code = exact(source);
    if source_code.is_none() {
        let items = ordered(config)
            .into_iter()
            .filter(|(code, name)| matches(source, code, name))
            .map(|(code, name)| {
                json!({
                    "title":format!("{} {code}",amount_answer.value),"subtitle":name,
                    "autocomplete":format!("{amount} {code} to "),"valid":false
                })
            })
            .collect::<Vec<_>>();
        return Ok(if items.is_empty() {
            vec![info(
                "No currency matches",
                "Try an ISO code or full currency name",
            )]
        } else {
            items
        });
    }
    let source = source_code.unwrap();
    rates.get(&source)?;
    let target_code = exact(target);
    let mut items = Vec::new();
    let mut first_error = None;
    for (code, name) in ordered(config).into_iter().filter(|(code, name)| {
        target_code
            .as_ref()
            .map(|target| target == *code)
            .unwrap_or_else(|| matches(target, code, name))
    }) {
        if target.is_empty() && *code == source {
            continue;
        }
        let mut answer = match math.eval_context(
            &format!("({normalized_amount}) {source} to {code}"),
            &mut context,
        ) {
            Ok(answer) => answer,
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
                continue;
            }
        };
        answer.detail = format!("{} → {name}", rates::names()[&source]);
        let question = format!("{amount} {source} to {code}");
        let mut row = answer.item(&question, &rates.status().1);
        row["autocomplete"] = question.into();
        items.push(row);
    }
    if items.is_empty() {
        if let Some(e) = first_error {
            return Err(e);
        }
        items.push(info(
            "No target currency matches",
            "Try a currency code or full name",
        ));
    }
    Ok(items)
}
