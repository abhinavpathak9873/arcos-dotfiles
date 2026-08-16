/// Converts streamed assistant Markdown into text suitable for a local speech
/// engine. Content is not summarized: only notation that is harmful when read
/// aloud is removed or replaced.
pub fn sanitize_for_speech(input: &str) -> String {
    let without_fences = remove_fenced_code(input);
    let mut output = String::with_capacity(without_fences.len());
    let chars: Vec<char> = without_fences.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '!' && chars.get(index + 1) == Some(&'[') {
            if let Some((label, next)) = markdown_link(&chars, index + 1) {
                output.push_str(&label);
                index = next;
                continue;
            }
        }
        if chars[index] == '[' {
            if let Some((label, next)) = markdown_link(&chars, index) {
                output.push_str(&label);
                index = next;
                continue;
            }
        }
        if starts_url(&chars, index) {
            while index < chars.len() && !chars[index].is_whitespace() {
                index += 1;
            }
            output.push_str(" link ");
            continue;
        }
        let character = chars[index];
        if !matches!(character, '`' | '*' | '_' | '#' | '|' | '~') && !is_emoji(character) {
            output.push(character);
        }
        index += 1;
    }
    collapse_whitespace(&output)
}

/// Yields only complete sentences. The returned remainder is held until more
/// Hermes text arrives, preventing the TTS engine from speaking fragments and
/// then repeating them when a stream chunk is extended.
pub fn take_complete_sentences(input: &str, flush: bool) -> (Vec<String>, String) {
    let mut sentences = Vec::new();
    let mut start = 0;
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    for (position, (byte, character)) in chars.iter().enumerate() {
        let boundary = matches!(character, '.' | '?' | '!')
            && chars
                .get(position + 1)
                .map(|(_, next)| next.is_whitespace())
                .unwrap_or(true);
        if boundary {
            let end = byte + character.len_utf8();
            let sentence = input[start..end].trim();
            if !sentence.is_empty() {
                sentences.push(sentence.to_owned());
            }
            start = end;
        }
    }
    let remainder = input[start..].trim().to_owned();
    if flush && !remainder.is_empty() {
        sentences.push(remainder);
        (sentences, String::new())
    } else {
        (sentences, remainder)
    }
}

fn remove_fenced_code(input: &str) -> String {
    let mut in_fence = false;
    let mut output = String::new();
    for line in input.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            if !in_fence {
                output.push_str(" code omitted. ");
            }
            continue;
        }
        if !in_fence {
            output.push_str(line);
            output.push('\n');
        }
    }
    output
}

fn markdown_link(chars: &[char], start: usize) -> Option<(String, usize)> {
    let close_label = chars[start + 1..].iter().position(|value| *value == ']')? + start + 1;
    if chars.get(close_label + 1) != Some(&'(') {
        return None;
    }
    let close_url = chars[close_label + 2..]
        .iter()
        .position(|value| *value == ')')?
        + close_label
        + 2;
    Some((
        chars[start + 1..close_label].iter().collect(),
        close_url + 1,
    ))
}

fn starts_url(chars: &[char], index: usize) -> bool {
    let tail: String = chars[index..chars.len().min(index + 8)].iter().collect();
    tail.starts_with("https://") || tail.starts_with("http://")
}

fn is_emoji(character: char) -> bool {
    ('\u{1F000}'..='\u{1FAFF}').contains(&character)
        || ('\u{2600}'..='\u{27BF}').contains(&character)
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_markdown_code_urls_emoji_and_tables() {
        let text = "# Result ✨\nSee [the docs](https://example.test) or https://example.test/raw.\n| A | B |\n```rust\nfn main() {}\n```";
        let spoken = sanitize_for_speech(text);
        assert_eq!(spoken, "Result See the docs or link A B code omitted.");
        assert!(!spoken.contains("https"));
    }

    #[test]
    fn schedules_each_sentence_once() {
        let (first, pending) = take_complete_sentences("Hello there. This is", false);
        assert_eq!(first, ["Hello there."]);
        let (second, pending) =
            take_complete_sentences(&format!("{pending} complete! Last"), false);
        assert_eq!(second, ["This is complete!"]);
        let (third, pending) = take_complete_sentences(&pending, true);
        assert_eq!(third, ["Last"]);
        assert!(pending.is_empty());
    }
}
