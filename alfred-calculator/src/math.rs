use crate::{
    config::Config,
    rates::{self, Rates},
    Answer, Result,
};
use fend_core::{Context, CustomUnitAttribute as Unit, Interrupt};
use regex::Captures;
use std::time::{Duration, Instant};

struct Deadline(Instant);
impl Interrupt for Deadline {
    fn should_interrupt(&self) -> bool {
        Instant::now() > self.0
    }
}
pub struct Math {
    pub config: Config,
    pub rates: Rates,
}
impl Math {
    pub fn new(config: &Config, rates: &Rates) -> Self {
        Self {
            config: config.clone(),
            rates: rates.clone(),
        }
    }
    pub fn context(&self, ppi: u32) -> Result<Context> {
        let mut ctx = Context::new();
        ctx.set_exchange_rate_handler_v2(self.rates.clone());
        for code in rates::names().keys() {
            ctx.define_custom_unit_v1(code, code, "$CURRENCY", &Unit::None);
            ctx.define_custom_unit_v1(
                &code.to_lowercase(),
                &code.to_lowercase(),
                code,
                &Unit::Alias,
            );
        }
        ctx.define_custom_unit_v1("px", "px", &format!("inch / {ppi}"), &Unit::None);
        ctx.define_custom_unit_v1(
            "workday",
            "workdays",
            &format!("{} hours", self.config.work_hours),
            &Unit::None,
        );
        ctx.define_custom_unit_v1(
            "workweek",
            "workweeks",
            &format!("{} hours", self.config.work_hours * 5),
            &Unit::None,
        );
        let defs="cot = x: 1/tan(x); csc = x: 1/sin(x); sec = x: 1/cos(x); coth = x: 1/tanh(x); csch = x: 1/sinh(x); sech = x: 1/cosh(x); acot = x: pi/2-atan(x); acsc = x: asin(1/x); asec = x: acos(1/x); acoth = x: atanh(1/x); acsch = x: asinh(1/x); asech = x: acosh(1/x)";
        fend_core::evaluate_with_interrupt(
            defs,
            &mut ctx,
            &Deadline(Instant::now() + Duration::from_millis(200)),
        )?;
        Ok(ctx)
    }
    pub fn normalize(&self, input: &str) -> String {
        let input = input
            .replace('−', "-")
            .replace('×', "*")
            .replace('÷', "/")
            .replace('π', "pi")
            .replace('√', "sqrt ");
        // Date parsing occurs before this numeric normalization.
        let input = re!(r"\d[\d.,\x{00A0}\x{202F}]*").replace_all(&input, |c: &Captures| {
            let n = c[0].replace(['\u{00a0}', '\u{202f}'], "");
            if self.config.comma {
                n.replace('.', "").replace(',', ".")
            } else {
                n.replace(',', "")
            }
        });
        let input = re!(r"(\d+(?:\.\d+)?)([kKMB])\b").replace_all(&input, |c: &Captures| {
            let scale = match &c[2] {
                "k" | "K" => 1000u64,
                "M" => 1_000_000,
                _ => 1_000_000_000,
            };
            format!("({}*{scale})", &c[1])
        });
        let input = re!(r"(?i)\b(USD|EUR|GBP|IDR|JPY|AUD|CAD|SGD|BTC|ETH|SOL)(\d+(?:\.\d+)?)")
            .replace_all(&input, "$2 $1");
        let mut input = input.into_owned();
        for (symbol, code) in [
            ("$", "USD "),
            ("€", "EUR "),
            ("£", "GBP "),
            ("¥", "JPY "),
            ("₿", "BTC "),
        ] {
            input = input.replace(symbol, code);
        }
        // Prefix currency notation becomes amount × currency, preserving groups.
        input = re!(r"\b([A-Z]{3,4})\s*(\d+(?:\.\d+)?|\([^)]*\))")
            .replace_all(&input, "$2 $1")
            .into_owned();
        for (pattern, replacement) in [
            (r"(?i)\bsquare root of\s+", "sqrt "),
            (r"(?i)\bcube root of\s+", "cbrt "),
            (r"(?i)\s+power\s+", " ^ "),
            (r"(?i)\bmins\b", "minutes"),
            (r"(?i)\bhrs\b", "hours"),
            (r"(?i)\bsecs\b", "seconds"),
            (r"(?i)\brupiahs?\b", "IDR"),
            (r"(?i)\beuros?\b", "EUR"),
            (r"(?i)\bbitcoins?\b", "BTC"),
            (r"(?i)\bethereum\b", "ETH"),
            (r"(?i)\bdollars?\b", "USD"),
        ] {
            input = regex::Regex::new(pattern)
                .unwrap()
                .replace_all(&input, replacement)
                .into_owned();
        }
        input
    }
    pub fn eval_normalized(&self, expr: &str, ppi: u32) -> Result<Answer> {
        self.eval_context(expr, &mut self.context(ppi)?)
    }
    pub fn eval_context(&self, expr: &str, ctx: &mut Context) -> Result<Answer> {
        self.eval_with_precision(expr, ctx, self.config.precision)
    }
    fn eval_with_precision(
        &self,
        expr: &str,
        ctx: &mut Context,
        precision: usize,
    ) -> Result<Answer> {
        let expr = crate::percent::rewrite(expr)?;
        if expr.len() > 2048 {
            return Err("Expression is too long (maximum 2,048 bytes)".into());
        }
        let mut depth = 0;
        for c in expr.chars() {
            if c == '(' {
                depth += 1;
                if depth > 64 {
                    return Err("Expression is nested too deeply".into());
                }
            } else if c == ')' {
                depth -= 1;
            }
        }
        let deadline = Deadline(Instant::now() + Duration::from_millis(400));
        let value = fend_core::evaluate_with_interrupt(
            &format!("({expr}) to {precision} dp"),
            ctx,
            &deadline,
        )?
        .get_main_result()
        .to_string();
        let raw = fend_core::evaluate_with_interrupt("@plain_number ans", ctx, &deadline)?
            .get_main_result()
            .to_string();
        if value.len() > 8192 {
            return Err("Result is too large to display".into());
        }
        // Reuse the engine's numeric value and units, avoiding f64 precision loss.
        let rounded = if re!(r"^-?\d+(?:\.\d+)?$").is_match(&raw) {
            fend_core::evaluate_with_interrupt("ans to 0 dp", ctx, &deadline)
                .ok()
                .map(|result| {
                    let value = result.get_main_result();
                    self.config
                        .format(value.strip_prefix("approx. ").unwrap_or(value))
                })
        } else {
            None
        };
        Ok(Answer {
            value: self.config.format(&value),
            raw,
            detail: String::new(),
            rounded,
        })
    }
    pub fn number(&self, expr: &str) -> Result<f64> {
        // Display precision must not round intermediate duration calculations.
        let answer = self.eval_with_precision(
            expr,
            &mut self.context(self.config.ppi)?,
            self.config.precision.max(12),
        )?;
        let n: f64 = answer.raw.parse().map_err(|_| "Expected a real number")?;
        if !n.is_finite() {
            return Err("Result is not finite".into());
        }
        Ok(n)
    }
    pub fn evaluate(&self, input: &str) -> Result<Vec<Answer>> {
        let expr = self.normalize(input);
        if let Some(c) = re!(r"(?i)^(.+?)\s+(?:in|to)\s+timespan$").captures(&expr) {
            let seconds = self.number(&format!("({}) to seconds", &c[1]))?;
            if seconds.abs() > 1e15 {
                return Err("Timespan is too large".into());
            }
            return Ok(vec![Answer {
                value: timespan(seconds),
                raw: decimal(seconds, self.config.precision),
                detail: "Raw: seconds".into(),
                rounded: None,
            }]);
        }
        if let Some(c) = re!(r"(?i)^(.+?)\s+at\s+(\d+)\s*(?:ppi|dpi)$").captures(&expr) {
            let ppi: u32 = c[2].parse()?;
            if ppi == 0 || ppi > 9600 {
                return Err("PPI must be between 1 and 9,600".into());
            }
            let mut a = self.eval_normalized(&c[1], ppi)?;
            a.detail = format!("{ppi} ppi");
            return Ok(vec![a]);
        }
        if let Some(c) = re!(r"(?i)^(.+?)%\s+off\s+(.+)$").captures(&expr) {
            let a = self.eval_normalized(
                &format!("({}) * (1 - ({})/100)", &c[2], &c[1]),
                self.config.ppi,
            )?;
            return Ok(vec![a]);
        }
        if let Some(c) = re!(r"(?i)^(.+?)%\s+tip\s+on\s+(.+)$").captures(&expr) {
            let mut tip =
                self.eval_normalized(&format!("({}) * ({})/100", &c[2], &c[1]), self.config.ppi)?;
            tip.detail = "Tip".into();
            let mut total = self.eval_normalized(
                &format!("({}) * (1 + ({})/100)", &c[2], &c[1]),
                self.config.ppi,
            )?;
            total.detail = "Total with tip".into();
            return Ok(vec![tip, total]);
        }
        if let Some(c) = re!(r"(?i)^ratio of\s+([+-]?\d+)\s+to\s+([+-]?\d+)$").captures(&expr) {
            let a: i64 = c[1].parse()?;
            let b: i64 = c[2].parse()?;
            if b == 0 {
                return Err("The second ratio value cannot be zero".into());
            }
            let (mut x, mut y) = (a.unsigned_abs(), b.unsigned_abs());
            while y != 0 {
                (x, y) = (y, x % y);
            }
            let ratio = format!(
                "{}:{}",
                i128::from(a) / i128::from(x),
                i128::from(b) / i128::from(x)
            );
            let mut decimal = self.eval_normalized(&format!("({a})/({b})"), self.config.ppi)?;
            decimal.detail = "Decimal".into();
            return Ok(vec![
                Answer {
                    value: ratio.clone(),
                    raw: ratio,
                    detail: "Ratio".into(),
                    rounded: None,
                },
                decimal,
            ]);
        }
        if let Some(c) =
            re!(r"(?i)^(.+?)\s+(?:invested\s+)?at\s+(.+?)%\s+(?:after|for)\s+(\d+)\s+years?$")
                .captures(&expr)
        {
            let mut a = self.eval_normalized(
                &format!("({})*(1+({})/100)^{}", &c[1], &c[2], &c[3]),
                self.config.ppi,
            )?;
            a.detail = "Annual compounding · no contributions or fees".into();
            return Ok(vec![a]);
        }
        let mut answer = self.eval_normalized(&expr, self.config.ppi)?;
        answer.detail = if expr.contains("workday") || expr.contains("workweek") {
            format!("{}h/day · 5 days/week", self.config.work_hours)
        } else {
            String::new()
        };
        Ok(vec![answer])
    }
}
pub fn decimal(n: f64, precision: usize) -> String {
    let value = format!("{n:.precision$}");
    let value = if value.contains('.') {
        value.trim_end_matches('0').trim_end_matches('.')
    } else {
        &value
    };
    if value == "-0" || value.is_empty() {
        "0".into()
    } else {
        value.into()
    }
}
pub fn timespan(seconds: f64) -> String {
    let mut remaining = seconds.abs();
    let mut parts = Vec::new();
    for (size, label) in [(86400.0, "day"), (3600.0, "hour"), (60.0, "minute")] {
        let n = (remaining / size).floor() as u64;
        remaining -= n as f64 * size;
        if n > 0 {
            parts.push(format!("{n} {label}{}", if n == 1 { "" } else { "s" }));
        }
    }
    if remaining >= 0.000001 || parts.is_empty() {
        parts.push(format!("{} seconds", decimal(remaining, 6)));
    }
    format!(
        "{}{}",
        if seconds < 0.0 { "−" } else { "" },
        parts.join(" ")
    )
}
