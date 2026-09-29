//! Calculator-style percent operators, before fend's unit-aware evaluation.
//! Keep atoms opaque so unit names and functions still belong to fend's parser.
use crate::Result;

struct Expr {
    text: String,
    percent: bool,
}
impl Expr {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            percent: false,
        }
    }
}

pub fn rewrite(input: &str) -> Result<String> {
    if !input.contains('%') {
        return Ok(input.into());
    }
    if input.len() > 2048 {
        return Err("Expression is too long".into());
    }
    Ok(parse(input.trim(), 0)?.text)
}

/// Positions outside parentheses/strings, with bounded, validated grouping.
fn top_level(input: &str) -> Result<Vec<(usize, char)>> {
    let mut positions = Vec::new();
    let mut depth = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (i, c) in input.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                quoted = false;
            }
            continue;
        }
        if c == '"' {
            quoted = true;
            continue;
        }
        if c == '(' {
            if depth == 0 {
                positions.push((i, c));
            }
            depth += 1;
            if depth > 64 {
                return Err("Expression is nested too deeply".into());
            }
        } else if c == ')' {
            depth -= 1;
            if depth < 0 {
                return Err("Unmatched closing parenthesis".into());
            }
        } else if depth == 0 {
            positions.push((i, c));
        }
    }
    if depth != 0 || quoted {
        return Err("Finish the expression's parentheses or quoted text".into());
    }
    Ok(positions)
}

fn binary_sign(input: &str, i: usize) -> bool {
    let before = input[..i].trim_end();
    let Some(previous) = before.chars().next_back() else {
        return false;
    };
    if "+-*/^=(,;:".contains(previous) {
        return false;
    }
    // The sign in scientific notation is part of the numeric atom.
    if matches!(previous, 'e' | 'E')
        && before[..before.len() - 1]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_digit() || c == '.')
    {
        return false;
    }
    true
}

fn parse(input: &str, depth: usize) -> Result<Expr> {
    if depth > 64 {
        return Err("Percentage expression is nested too deeply".into());
    }
    let input = input.trim();
    if !input.contains('%') {
        return Ok(Expr::plain(input));
    }
    let positions = top_level(input)?;
    // Preserve statement boundaries, assignment and explicit unit conversion.
    for token in [';', '='] {
        if let Some(&(i, _)) = positions.iter().find(|(_, c)| *c == token) {
            let left = parse(&input[..i], depth + 1)?;
            let right = parse(&input[i + 1..], depth + 1)?;
            return Ok(Expr::plain(format!("{} {token} {}", left.text, right.text)));
        }
    }
    for connector in [" to ", " in ", " as "] {
        if let Some((i, _)) = positions
            .iter()
            .rev()
            .find(|(i, _)| input[*i..].starts_with(connector))
        {
            let left = parse(&input[..*i], depth + 1)?;
            return Ok(Expr::plain(format!(
                "({}){connector}{}",
                left.text,
                &input[*i + connector.len()..]
            )));
        }
    }
    // Additive operators associate left-to-right. A percent on the right
    // adjusts the complete amount on the left without duplicating its AST.
    if let Some(&(i, op)) = positions
        .iter()
        .rev()
        .find(|(i, c)| matches!(c, '+' | '-') && binary_sign(input, *i))
    {
        let left = parse(&input[..i], depth + 1)?;
        let right = parse(&input[i + 1..], depth + 1)?;
        let text = if right.percent {
            format!("({}) * (1 {op} ({}))", left.text, right.text)
        } else {
            format!("({}) {op} ({})", left.text, right.text)
        };
        return Ok(Expr::plain(text));
    }
    if let Some(&(i, op)) = positions.iter().rev().find(|(i, c)| {
        match c {
            '*' => !input[..*i].ends_with('*') && !input[*i + 1..].starts_with('*'),
            '/' => true,
            // '%' between two operands remains modulo, just like fend.
            '%' => input[*i + 1..]
                .trim_start()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '.'),
            _ => false,
        }
    }) {
        let left = parse(&input[..i], depth + 1)?;
        let right = parse(&input[i + 1..], depth + 1)?;
        return Ok(Expr::plain(format!(
            "({}) {} ({})",
            left.text,
            if op == '%' { "mod" } else { &input[i..i + 1] },
            right.text
        )));
    }
    if let Some(&(i, _)) = positions
        .iter()
        .find(|(i, c)| *c == '^' || (*c == '*' && input[*i..].starts_with("**")))
    {
        let len = if input[i..].starts_with("**") { 2 } else { 1 };
        let left = parse(&input[..i], depth + 1)?;
        let right = parse(&input[i + len..], depth + 1)?;
        return Ok(Expr::plain(format!("({}) ^ ({})", left.text, right.text)));
    }
    if let Some((i, _)) = positions
        .iter()
        .find(|(i, _)| input[*i..].starts_with(" of "))
    {
        let left = parse(&input[..*i], depth + 1)?;
        let right = parse(&input[*i + 4..], depth + 1)?;
        if left.percent {
            return Ok(Expr::plain(format!("({}) * ({})", left.text, right.text)));
        }
    }
    if input.starts_with(['+', '-']) {
        let inner = parse(&input[1..], depth + 1)?;
        return Ok(Expr {
            text: format!("{}({})", &input[..1], inner.text),
            percent: inner.percent,
        });
    }
    if let Some(base) = input.strip_suffix('%').filter(|s| !s.trim().is_empty()) {
        return Ok(Expr {
            text: format!("({}) / 100", parse(base, depth + 1)?.text),
            percent: true,
        });
    }
    // Transform nested arithmetic in function arguments and grouped operands.
    let mut output = String::new();
    let mut start = 0;
    for &(open, c) in &positions {
        if c != '(' {
            continue;
        }
        let mut nesting = 1;
        let mut close = None;
        let mut quoted = false;
        let mut escaped = false;
        for (offset, c) in input[open + 1..].char_indices() {
            if quoted {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    quoted = false;
                }
                continue;
            }
            if c == '"' {
                quoted = true;
            } else if c == '(' {
                nesting += 1;
            } else if c == ')' {
                nesting -= 1;
            }
            if nesting == 0 {
                close = Some(open + 1 + offset);
                break;
            }
        }
        let close = close.ok_or("Unclosed parentheses")?;
        let inner = parse(&input[open + 1..close], depth + 1)?;
        if open == 0 && close + 1 == input.len() {
            return Ok(Expr {
                text: format!("({})", inner.text),
                percent: inner.percent,
            });
        }
        output.push_str(&input[start..open]);
        output.push_str(&format!("({})", inner.text));
        start = close + 1;
    }
    output.push_str(&input[start..]);
    Ok(Expr::plain(output))
}
