//! "Did you mean" helpers.

pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Returns the closest candidate if it is reasonably similar.
pub fn closest<'a, I>(word: &str, candidates: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut best: Option<(&str, usize)> = None;
    for c in candidates {
        let d = levenshtein(&word.to_lowercase(), &c.to_lowercase());
        if best.map_or(true, |(_, bd)| d < bd) {
            best = Some((c, d));
        }
    }
    let limit = (word.chars().count() / 3).max(2);
    best.and_then(|(c, d)| if d <= limit { Some(c) } else { None })
}

/// Builds a "did you mean" suggestion, or lists a few valid options.
pub fn did_you_mean<'a, I>(word: &str, candidates: I) -> String
where
    I: IntoIterator<Item = &'a str> + Clone,
{
    if let Some(c) = closest(word, candidates.clone()) {
        return format!("did you mean \"{}\"?", c);
    }
    let list: Vec<&str> = candidates.into_iter().take(12).collect();
    format!("valid options include: {}", list.join(", "))
}
