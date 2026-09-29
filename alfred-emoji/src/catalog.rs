const DATA: &str = include_str!("../data/emoji.tsv");

pub struct Emoji {
    pub glyph: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    pub subgroup: &'static str,
    pub keywords: Vec<&'static str>,
}

impl Emoji {
    pub fn icon(&self) -> String {
        let hex: Vec<String> = self.glyph.chars().map(|c| format!("{:x}", c as u32)).collect();
        format!("icons/{}.png", hex.join("-"))
    }

    pub fn title(&self) -> String {
        let mut chars = self.name.chars();
        chars
            .next()
            .map(|c| c.to_uppercase().chain(chars).collect())
            .unwrap_or_default()
    }

    fn words(&self) -> impl Iterator<Item = (&'static str, bool)> + '_ {
        let name = self.name.split(|c: char| !c.is_alphanumeric()).map(|w| (w, true));
        let keywords = self
            .keywords
            .iter()
            .flat_map(|k| k.split(|c: char| !c.is_alphanumeric()))
            .map(|w| (w, false));
        name.chain(keywords).filter(|(w, _)| !w.is_empty())
    }
}

pub struct Catalog {
    pub emoji: Vec<Emoji>,
}

impl Catalog {
    pub fn load() -> Self {
        let emoji = DATA
            .lines()
            .filter(|l| !l.is_empty())
            .map(|line| {
                let mut f = line.split('\t');
                let mut next = || f.next().unwrap_or("");
                Emoji {
                    glyph: next(),
                    name: next(),
                    group: next(),
                    subgroup: next(),
                    keywords: next().split('|').filter(|k| !k.is_empty()).collect(),
                }
            })
            .collect();
        Self { emoji }
    }

    pub fn find(&self, glyph: &str) -> Option<usize> {
        self.emoji.iter().position(|e| e.glyph == glyph)
    }

    /// Every query word must prefix a word in the name or CLDR keywords.
    /// Returns (index, is_exact_name) ordered by relevance, then catalog order.
    pub fn keyword_search(&self, query: &str) -> Vec<(usize, bool)> {
        let query = query.trim().to_lowercase();
        let tokens: Vec<&str> = query.split_whitespace().collect();
        if tokens.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(u32, usize, bool)> = self
            .emoji
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                if e.name == query || e.glyph == query {
                    return Some((u32::MAX, i, true));
                }
                let mut total = 0;
                for token in &tokens {
                    let best = e
                        .words()
                        .map(|(w, in_name)| {
                            let w = w.to_lowercase();
                            match (w == *token, w.starts_with(token), in_name) {
                                (true, _, true) => 4,
                                (true, _, false) => 3,
                                (_, true, true) => 2,
                                (_, true, false) => 1,
                                _ => 0,
                            }
                        })
                        .max()
                        .unwrap_or(0);
                    if best == 0 {
                        return None;
                    }
                    total += best;
                }
                Some((total, i, false))
            })
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        hits.into_iter().map(|(_, i, exact)| (i, exact)).collect()
    }
}
