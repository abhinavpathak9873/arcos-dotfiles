use arc_protocol::{ModelRoute, ModelTier};

/// Selects the least expensive model that still matches the work. The route is
/// persisted with the utterance so a recovered service never silently changes
/// models half way through a task.
pub fn route(
    text: &str,
    requested_model: Option<&str>,
    requested_effort: Option<&str>,
) -> ModelRoute {
    let prompt = text.trim();
    let lower = prompt.to_lowercase();
    let requested = requested_model.unwrap_or("auto").to_lowercase();
    let effort = requested_effort.unwrap_or("auto");
    let explicit_sol = requested.contains("sol") || requested_in_prompt(&lower, "sol");
    let explicit_terra = requested.contains("terra") || requested_in_prompt(&lower, "terra");
    let explicit_luna =
        (requested != "auto" && requested.contains("luna")) || requested_in_prompt(&lower, "luna");
    let deep_terms = [
        "architect",
        "system design",
        "redesign",
        "migration",
        "security",
        "vulnerability",
        "threat model",
        "root cause",
        "race condition",
        "deadlock",
        "memory leak",
        "data loss",
        "large refactor",
        "across the repo",
        "across the codebase",
        "production incident",
        "performance bottleneck",
        "implement everything",
        "end-to-end",
        "comprehensive",
    ];
    let coding_terms = [
        " code",
        "coding",
        "implement",
        "refactor",
        "debug",
        " fix",
        "test",
        "build",
        "repository",
        "codebase",
        "typescript",
        "rust",
        "python",
    ];
    let deep = explicit_sol
        || (!explicit_terra
            && !explicit_luna
            && (prompt.len() > 700 || deep_terms.iter().any(|term| lower.contains(term))));
    if deep {
        return ModelRoute {
            model: "gpt-5.6-sol".into(),
            effort: normalize_effort(effort, "high"),
            tier: ModelTier::Deep,
            reason: if explicit_sol {
                "User selected Sol"
            } else {
                "Complex task requires deeper analysis"
            }
            .into(),
            delegate_to_codex: is_substantial_repository_work(&lower),
        };
    }
    let coding =
        explicit_terra || (!explicit_luna && coding_terms.iter().any(|term| lower.contains(term)));
    if coding {
        return ModelRoute {
            model: "gpt-5.6-terra".into(),
            effort: normalize_effort(effort, "medium"),
            tier: ModelTier::Fast,
            reason: if explicit_terra {
                "User selected Terra"
            } else {
                "Routine tool or coding path"
            }
            .into(),
            delegate_to_codex: is_substantial_repository_work(&lower),
        };
    }
    let simple = prompt.len() < 240
        && [
            "open ", "launch ", "start ", "focus ", "close ", "what is ", "who is ", "list ",
            "stop", "yes", "no", "thanks",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix));
    ModelRoute {
        model: "gpt-5.6-luna".into(),
        effort: normalize_effort(effort, if simple { "low" } else { "medium" }),
        tier: ModelTier::Instant,
        reason: if explicit_luna {
            "User selected Luna"
        } else if simple {
            "Instant conversation and desktop action path"
        } else {
            "Responsive everyday path"
        }
        .into(),
        delegate_to_codex: false,
    }
}

fn requested_in_prompt(prompt: &str, model: &str) -> bool {
    ["use ", "switch to ", "run on "]
        .iter()
        .any(|prefix| prompt.contains(&format!("{prefix}{model}")))
}

fn normalize_effort(value: &str, fallback: &str) -> String {
    match value {
        "low" | "medium" | "high" => value.into(),
        _ => fallback.into(),
    }
}

fn is_substantial_repository_work(prompt: &str) -> bool {
    let repository = [
        "repository",
        "repo",
        "codebase",
        "project",
        "crate",
        "package",
    ]
    .iter()
    .any(|term| prompt.contains(term));
    let mutation = [
        "implement",
        "change",
        "fix",
        "refactor",
        "migrate",
        "build",
        "edit",
        "redesign",
        "architect",
    ]
    .iter()
    .any(|term| prompt.contains(term));
    repository && mutation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_conversation_to_luna() {
        let selected = route("What is on my calendar?", None, None);
        assert_eq!(selected.model, "gpt-5.6-luna");
        assert_eq!(selected.effort, "low");
    }

    #[test]
    fn routes_routine_code_to_terra_and_delegates_repo_mutation() {
        let selected = route("Implement the fix in this repository", None, None);
        assert_eq!(selected.model, "gpt-5.6-terra");
        assert!(selected.delegate_to_codex);
    }

    #[test]
    fn routes_architecture_to_sol() {
        let selected = route(
            "Redesign the security architecture across the codebase",
            None,
            None,
        );
        assert_eq!(selected.model, "gpt-5.6-sol");
        assert_eq!(selected.effort, "high");
        assert!(selected.delegate_to_codex);
    }
}
