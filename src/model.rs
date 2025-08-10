use std::collections::{HashMap, HashSet};
use std::path::{PathBuf, Path};
use serde::{Deserialize, Serialize};
use super::lexer::Lexer;
use std::time::SystemTime;

type DocFreq = HashMap<String, usize>;
type TermFreq = HashMap<String, usize>;

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct Model {
    pub docs: HashMap<PathBuf, Doc>,
    pub df: DocFreq,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Doc {
    count: usize,
    tf: TermFreq,
    last_modified: SystemTime,
    #[serde(default)]
    positions: HashMap<String, Vec<usize>>, // token -> positions in sequence
}

impl Model {
    fn remove_document(&mut self, file_path: &Path) {
        if let Some(doc) = self.docs.remove(file_path) {
            for t in doc.tf.keys() {
                if let Some(f) = self.df.get_mut(t) {
                    *f -= 1;
                }
            }
        }
    }

    pub fn requires_reindexing(&mut self, file_path: &Path, last_modified: SystemTime) -> bool {
        if let Some(doc) = self.docs.get(file_path) {
            return doc.last_modified < last_modified;
        }
        return true;
    }

    pub fn search_query(&self, query: &[char]) -> Vec<(PathBuf, f32)> {
        let mut result = Vec::new();
        let tokens = Lexer::new(&query).collect::<Vec<_>>();
        // Distinct token set for multi-term coverage boost
        let distinct: HashSet<&str> = tokens.iter().map(|s| s.as_str()).collect();
        let distinct_len = distinct.len().max(1) as f32;
        for (path, doc) in &self.docs {
            let mut rank = 0f32;
            for token in &tokens {
                rank += compute_tf(token, doc) * compute_idf(&token, self.docs.len(), &self.df);
            }
            if distinct.len() > 1 {
                // Count how many distinct query tokens are present in this doc
                let present = distinct.iter().filter(|t| doc.tf.contains_key(**t)).count() as f32;
                let coverage = present / distinct_len; // 0..1
                // New scheme: strong penalty for partial coverage, bonus for full coverage
                const FULL_COVER_BONUS: f32 = 0.5; // extra 50% if all terms present
                const PARTIAL_EXP: f32 = 2.0; // coverage exponent for partial docs
                let coverage_factor = if coverage >= 1.0 {
                    1.0 + FULL_COVER_BONUS
                } else {
                    // (coverage^exp) shrinks rank for missing terms; ensures multi-term intent respected
                    coverage.powf(PARTIAL_EXP)
                };
                rank *= coverage_factor;
            }
            // Phrase boost: if full ordered sequence of tokens appears contiguously
            if tokens.len() > 1 && phrase_in_doc(&tokens, doc) {
                const PHRASE_BOOST: f32 = 2.0; // multiplicative boost for exact phrase
                rank *= PHRASE_BOOST;
            }
            // TODO: investigate the sources of NaN
            if !rank.is_nan() {
                result.push((path.clone(), rank));
            }
        }
        result.sort_by(|(_, rank1), (_, rank2)| rank1.partial_cmp(rank2).expect(&format!("{rank1} and {rank2} are not comparable")));
        result.reverse();
        result
    }

    pub fn add_document(&mut self, file_path: PathBuf, last_modified: SystemTime, content: &[char]) {
        self.remove_document(&file_path);

        let mut tf = TermFreq::new();

        let mut count = 0;
        let mut positions: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, t) in Lexer::new(content).enumerate() {
            if let Some(f) = tf.get_mut(&t) {
                *f += 1;
            } else {
                tf.insert(t.clone(), 1);
            }
            positions.entry(t).or_default().push(idx);
            count += 1;
        }

        for t in tf.keys() {
            if let Some(f) = self.df.get_mut(t) {
                *f += 1;
            } else {
                self.df.insert(t.to_string(), 1);
            }
        }

    self.docs.insert(file_path, Doc {count, tf, last_modified, positions});
    }
}

fn compute_tf(t: &str, doc: &Doc) -> f32 {
    let n = doc.count as f32;
    let m = doc.tf.get(t).cloned().unwrap_or(0) as f32;
    m / n
}

fn compute_idf(t: &str, n: usize, df: &DocFreq) -> f32 {
    let n = n as f32;
    let m = df.get(t).cloned().unwrap_or(1) as f32;
    (n / m).log10()
}

fn phrase_in_doc(tokens: &[String], doc: &Doc) -> bool {
    if tokens.is_empty() { return false; }
    // Quick reject if any token missing
    for t in tokens { if !doc.tf.contains_key(t) { return false; } }
    // Get candidate starting positions for first token
    if let Some(first_pos) = doc.positions.get(&tokens[0]) {
        // For each start, test consecutive positions
        'outer: for &start in first_pos {
            let mut expected = start + 1;
            for tok in &tokens[1..] {
                match doc.positions.get(tok) {
                    Some(pos_vec) => {
                        if !pos_vec.binary_search(&expected).is_ok() { continue 'outer; }
                        expected += 1;
                    }
                    None => continue 'outer,
                }
            }
            return true; // all matched consecutively
        }
    }
    false
}