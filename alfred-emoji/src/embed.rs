use crate::catalog::{Catalog, Emoji};
use crate::Result;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use std::{collections::HashMap, fs, path::Path};
use unicode_normalization::UnicodeNormalization;

pub const DIMS: usize = 384;
pub type Bits = [u64; DIMS / 64];

const QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";
const SHORTLIST: usize = 100;
const MAX_TOKENS: usize = 128;
const BATCH: usize = 64;

fn err(e: impl std::fmt::Display) -> String {
    format!("Embedding model error: {e}")
}

pub struct Embedder {
    model: BertModel,
    vocab: HashMap<String, u32>,
}

/// BERT basic tokenization: lowercase, strip accents, split on whitespace and punctuation.
pub fn basic_tokens(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    for c in text.to_lowercase().nfd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)) {
        let cjk = matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F);
        if c.is_whitespace() || c.is_control() || c.is_ascii_punctuation() || cjk || is_punctuation(c) {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            if !(c.is_whitespace() || c.is_control()) {
                words.push(c.to_string());
            }
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

fn is_punctuation(c: char) -> bool {
    matches!(c as u32, 0x2010..=0x2027 | 0x2030..=0x205E | 0x3000..=0x303F | 0xA1 | 0xBF)
}

impl Embedder {
    pub fn load(dir: &Path) -> Result<Self> {
        let read = |name: &str| {
            fs::read_to_string(dir.join(name)).map_err(|e| format!("Cannot read model/{name}: {e}"))
        };
        let config: Config = serde_json::from_str(&read("config.json")?).map_err(err)?;
        let vocab = read("vocab.txt")?.lines().zip(0u32..).map(|(t, i)| (t.to_string(), i)).collect();
        let weights = dir.join("model.safetensors");
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[weights], DType::F32, &Device::Cpu) }
            .map_err(err)?;
        Ok(Self { model: BertModel::load(vb, &config).map_err(err)?, vocab })
    }

    fn id(&self, token: &str) -> u32 {
        self.vocab.get(token).copied().unwrap_or(self.vocab["[UNK]"])
    }

    /// Greedy longest-match-first WordPiece, as in the reference BERT tokenizer.
    pub fn tokenize(&self, text: &str) -> Vec<u32> {
        let mut ids = vec![self.id("[CLS]")];
        for word in basic_tokens(text) {
            let chars: Vec<char> = word.chars().collect();
            let mut pieces = Vec::new();
            let mut start = 0;
            while start < chars.len() && chars.len() <= 100 {
                let piece = (start + 1..=chars.len()).rev().find_map(|end| {
                    let s: String = chars[start..end].iter().collect();
                    let s = if start > 0 { format!("##{s}") } else { s };
                    self.vocab.get(&s).map(|&id| (id, end))
                });
                let Some((id, end)) = piece else { break };
                pieces.push(id);
                start = end;
            }
            if start == chars.len() {
                ids.extend(pieces);
            } else {
                ids.push(self.id("[UNK]"));
            }
        }
        ids.truncate(MAX_TOKENS - 1);
        ids.push(self.id("[SEP]"));
        ids
    }

    /// Normalized CLS embeddings, the pooling bge models are trained with.
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for batch in texts.chunks(BATCH) {
            let tokens: Vec<Vec<u32>> = batch.iter().map(|t| self.tokenize(t)).collect();
            let width = tokens.iter().map(Vec::len).max().unwrap_or(0);
            let mut ids = Vec::with_capacity(batch.len() * width);
            let mut mask = Vec::with_capacity(batch.len() * width);
            for t in &tokens {
                ids.extend(t.iter().copied().chain(std::iter::repeat(0).take(width - t.len())));
                mask.extend((0..width).map(|i| u32::from(i < t.len())));
            }
            let shape = (batch.len(), width);
            let ids = Tensor::from_vec(ids, shape, &Device::Cpu).map_err(err)?;
            let mask = Tensor::from_vec(mask, shape, &Device::Cpu).map_err(err)?;
            let types = ids.zeros_like().map_err(err)?;
            let hidden = self.model.forward(&ids, &types, Some(&mask)).map_err(err)?;
            let cls = hidden.narrow(1, 0, 1).and_then(|t| t.squeeze(1)).map_err(err)?;
            for mut v in cls.to_vec2::<f32>().map_err(err)? {
                let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
                v.iter_mut().for_each(|x| *x /= norm);
                out.push(v);
            }
        }
        Ok(out)
    }
}

/// One bit per dimension: 1 when the weight is positive.
pub fn quantize(v: &[f32]) -> Bits {
    let mut bits = [0u64; DIMS / 64];
    for (i, &x) in v.iter().enumerate().take(DIMS) {
        if x > 0.0 {
            bits[i / 64] |= 1 << (i % 64);
        }
    }
    bits
}

pub fn hamming(a: &Bits, b: &Bits) -> u32 {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
}

/// Shortlists by Hamming distance, then rescores the shortlist with the
/// float query against each ±1 document vector.
pub fn rank(index: &[Bits], query: &[f32], limit: usize) -> Vec<(usize, f32)> {
    let q = quantize(query);
    let mut near: Vec<(u32, usize)> = index.iter().enumerate().map(|(i, b)| (hamming(&q, b), i)).collect();
    near.sort_unstable();
    let mut scored: Vec<(usize, f32)> = near
        .into_iter()
        .take(SHORTLIST)
        .map(|(_, i)| {
            let dot: f32 = query
                .iter()
                .enumerate()
                .map(|(d, x)| if index[i][d / 64] >> (d % 64) & 1 == 1 { *x } else { -x })
                .sum();
            (i, dot)
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(limit);
    scored
}

pub fn document(e: &Emoji) -> String {
    if !e.description.is_empty() {
        e.description.to_string()
    } else if e.keywords.is_empty() {
        e.name.to_string()
    } else {
        format!("{}: {}", e.name, e.keywords.join(", "))
    }
}

pub fn read_index(path: &Path, len: usize) -> Result<Vec<Bits>> {
    let bytes = fs::read(path).map_err(|e| format!("Cannot read model/emoji.bin: {e}"))?;
    if bytes.len() != len * DIMS / 8 {
        return Err("model/emoji.bin does not match the emoji catalog. Rebuild the workflow.".into());
    }
    Ok(bytes
        .chunks_exact(DIMS / 8)
        .map(|c| std::array::from_fn(|w| u64::from_le_bytes(c[w * 8..w * 8 + 8].try_into().unwrap())))
        .collect())
}

pub fn search(catalog: &Catalog, dir: &Path, query: &str, limit: usize) -> Result<Vec<(usize, f32)>> {
    let index = read_index(&dir.join("emoji.bin"), catalog.emoji.len())?;
    let embedder = Embedder::load(dir)?;
    let q = embedder.embed(&[format!("{QUERY_PREFIX}{}", query.trim())])?.remove(0);
    Ok(rank(&index, &q, limit))
}

/// Writes float16 weights and the binary emoji index from a float32 bge checkout.
pub fn build(catalog: &Catalog, src: &Path, out: &Path) -> Result<()> {
    fs::create_dir_all(out).map_err(err)?;
    for name in ["config.json", "vocab.txt"] {
        fs::copy(src.join(name), out.join(name)).map_err(|e| format!("Cannot copy {name}: {e}"))?;
    }
    let tensors = candle_core::safetensors::load(src.join("model.safetensors"), &Device::Cpu).map_err(err)?;
    let half = tensors
        .into_iter()
        .map(|(k, t)| t.to_dtype(DType::F16).map(|t| (k, t)))
        .collect::<candle_core::Result<HashMap<_, _>>>()
        .map_err(err)?;
    candle_core::safetensors::save(&half, out.join("model.safetensors")).map_err(err)?;
    let embedder = Embedder::load(out)?;
    let docs: Vec<String> = catalog.emoji.iter().map(document).collect();
    let bytes: Vec<u8> = embedder
        .embed(&docs)?
        .iter()
        .flat_map(|v| quantize(v))
        .flat_map(u64::to_le_bytes)
        .collect();
    fs::write(out.join("emoji.bin"), bytes).map_err(err)
}
