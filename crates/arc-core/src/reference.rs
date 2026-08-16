use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Evidence {
    ExactName,
    FuzzyName(f32),
    ActiveDocument,
    ProjectProximity,
    Recent(f32),
    Alias,
    ContentMatch(f32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub path: PathBuf,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Resolution {
    Attached {
        path: PathBuf,
        confidence: f32,
        reasons: Vec<String>,
    },
    Ambiguous {
        candidates: Vec<PathBuf>,
    },
    None,
}

#[derive(Debug, Clone)]
pub struct ReferenceResolver {
    pub attach_threshold: f32,
    pub ambiguity_margin: f32,
}

impl Default for ReferenceResolver {
    fn default() -> Self {
        Self {
            attach_threshold: 0.78,
            ambiguity_margin: 0.08,
        }
    }
}

impl ReferenceResolver {
    pub fn resolve(&self, mut candidates: Vec<Candidate>) -> Resolution {
        if candidates.is_empty() {
            return Resolution::None;
        }
        let mut scored: Vec<_> = candidates
            .drain(..)
            .map(|c| {
                let score = score(&c.evidence);
                (score, c)
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let (best_score, best) = &scored[0];
        if *best_score < self.attach_threshold {
            return Resolution::None;
        }
        if scored
            .get(1)
            .is_some_and(|(next, _)| best_score - next < self.ambiguity_margin)
        {
            return Resolution::Ambiguous {
                candidates: scored.into_iter().take(3).map(|(_, c)| c.path).collect(),
            };
        }
        Resolution::Attached {
            path: best.path.clone(),
            confidence: *best_score,
            reasons: best.evidence.iter().map(reason).collect(),
        }
    }
}

fn score(evidence: &[Evidence]) -> f32 {
    let miss = evidence
        .iter()
        .fold(1.0_f32, |remaining, item| remaining * (1.0 - weight(item)));
    (1.0 - miss).min(1.0)
}
fn weight(e: &Evidence) -> f32 {
    match e {
        Evidence::ExactName => 0.72,
        Evidence::FuzzyName(v) => 0.55 * v,
        Evidence::ActiveDocument => 0.68,
        Evidence::ProjectProximity => 0.34,
        Evidence::Recent(v) => 0.24 * v,
        Evidence::Alias => 0.8,
        Evidence::ContentMatch(v) => 0.45 * v,
    }
}
fn reason(e: &Evidence) -> String {
    match e {
        Evidence::ExactName => "filename matches your words".into(),
        Evidence::FuzzyName(_) => "filename sounds similar".into(),
        Evidence::ActiveDocument => "currently focused document".into(),
        Evidence::ProjectProximity => "inside the active project".into(),
        Evidence::Recent(_) => "recently used".into(),
        Evidence::Alias => "matches a learned alias".into(),
        Evidence::ContentMatch(_) => "content matches your request".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(path: &str, evidence: Vec<Evidence>) -> Candidate {
        Candidate {
            path: path.into(),
            evidence,
        }
    }
    #[test]
    fn attaches_a_clear_winner() {
        let r = ReferenceResolver::default().resolve(vec![
            c(
                "auth.ts",
                vec![Evidence::ExactName, Evidence::ActiveDocument],
            ),
            c("author.ts", vec![Evidence::FuzzyName(0.7)]),
        ]);
        assert!(
            matches!(r, Resolution::Attached { path, .. } if path == std::path::Path::new("auth.ts"))
        );
    }
    #[test]
    fn asks_when_close() {
        let r = ReferenceResolver::default().resolve(vec![
            c("a/auth.ts", vec![Evidence::Alias]),
            c("b/auth.ts", vec![Evidence::Alias]),
        ]);
        assert!(matches!(r, Resolution::Ambiguous { .. }));
    }
    #[test]
    fn avoids_low_confidence_attachment() {
        let r =
            ReferenceResolver::default().resolve(vec![c("maybe.txt", vec![Evidence::Recent(0.5)])]);
        assert_eq!(r, Resolution::None);
    }
}
