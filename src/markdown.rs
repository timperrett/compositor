use crate::model::{Directives, Unit};
use crate::AppError;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub metadata: BTreeMap<String, serde_yaml::Value>,
    pub body: String,
    pub units: Vec<Unit>,
}

pub fn parse_document(input: &str) -> Result<ParsedDocument, AppError> {
    let (metadata, body) = split_front_matter(input)?;
    let boundaries = top_level_boundaries(&body);
    let mut starts = vec![0];
    for (start, end) in boundaries {
        starts.push(end);
        let _ = start;
    }
    let mut ends: Vec<usize> = top_level_boundaries(&body)
        .into_iter()
        .map(|(start, _)| start)
        .collect();
    ends.push(body.len());
    let units = starts
        .into_iter()
        .zip(ends)
        .enumerate()
        .map(|(index, (start, end))| {
            let content = body[start..end].trim().to_string();
            let directives = parse_directives(&content)?;
            let visible = strip_directives(&content);
            let normalized_content = normalize(&visible);
            Ok(Unit {
                ordinal: index + 1,
                source_start: start,
                source_end: end,
                content,
                content_hash: content_hash(&normalized_content),
                word_count: normalized_content.split_whitespace().count(),
                normalized_content,
                directives,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(ParsedDocument {
        metadata,
        body,
        units,
    })
}

fn split_front_matter(
    input: &str,
) -> Result<(BTreeMap<String, serde_yaml::Value>, String), AppError> {
    let normalized = input.replace("\r\n", "\n");
    if !normalized.starts_with("---\n") {
        return Ok((BTreeMap::new(), normalized));
    }
    let Some(end) = normalized[4..].find("\n---\n") else {
        return Err(AppError::Config("unterminated YAML front matter".into()));
    };
    let end = end + 4;
    let yaml = &normalized[4..end];
    let metadata = serde_yaml::from_str(yaml)
        .map_err(|error| AppError::Config(format!("invalid YAML front matter: {error}")))?;
    Ok((metadata, normalized[end + 5..].to_string()))
}

fn top_level_boundaries(body: &str) -> Vec<(usize, usize)> {
    let parser = Parser::new_ext(body, Options::all()).into_offset_iter();
    let mut boundaries = Vec::new();
    let mut nesting = 0usize;
    for (event, range) in parser {
        match event {
            Event::Start(Tag::BlockQuote(_))
            | Event::Start(Tag::List(_))
            | Event::Start(Tag::CodeBlock(_)) => nesting += 1,
            Event::End(TagEnd::BlockQuote(_))
            | Event::End(TagEnd::List(_))
            | Event::End(TagEnd::CodeBlock) => nesting = nesting.saturating_sub(1),
            Event::Rule if nesting == 0 => boundaries.push((range.start, range.end)),
            _ => {}
        }
    }
    boundaries
}

fn parse_directives(content: &str) -> Result<Directives, AppError> {
    let mut result = Directives::default();
    for range in top_level_comment_ranges(content) {
        let comment = content[range.start + 4..range.end - 3].trim();
        if comment == "keep-with-next" {
            result.keep_with_next = true;
            continue;
        }
        let Some((name, value)) = comment.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim() {
            "anchor" => {
                if result.anchor.is_some() {
                    return Err(AppError::Config(
                        "multiple anchor directives in one unit".into(),
                    ));
                }
                if !valid_anchor(value) {
                    return Err(AppError::Config(format!("invalid anchor `{value}`")));
                }
                result.anchor = Some(value.into());
            }
            "art" => {
                if result.art.is_some() {
                    return Err(AppError::Config(
                        "multiple art directives in one unit".into(),
                    ));
                }
                result.art = Some(value.into());
            }
            "layout" => {
                if !matches!(
                    value,
                    "auto"
                        | "text-dominant"
                        | "art-dominant"
                        | "full-page"
                        | "full-spread"
                        | "facing-art"
                        | "spot-art"
                        | "illustration-only"
                ) {
                    return Err(AppError::Config(format!("unsupported layout `{value}`")));
                }
                result.layout = Some(value.into());
            }
            "unit" => {
                if !matches!(
                    value,
                    "narrative"
                        | "transition"
                        | "story-opening"
                        | "story-closing"
                        | "illustration-only"
                        | "blank"
                ) {
                    return Err(AppError::Config(format!("unsupported unit type `{value}`")));
                }
                result.unit_type = Some(value.into());
            }
            _ if name.trim().starts_with("anchor")
                || name.trim().starts_with("art")
                || name.trim().starts_with("layout")
                || name.trim().starts_with("unit") =>
            {
                return Err(AppError::Config(format!(
                    "unsupported directive `{}`",
                    name.trim()
                )))
            }
            _ => {}
        }
    }
    Ok(result)
}

pub fn valid_anchor(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub fn strip_directives(input: &str) -> String {
    let removals = top_level_comment_ranges(input)
        .into_iter()
        .filter(|range| is_directive_comment(input[range.start + 4..range.end - 3].trim()))
        .collect::<Vec<_>>();
    if removals.is_empty() {
        return input.to_owned();
    }
    let mut output = String::new();
    let mut offset = 0;
    for range in removals {
        output.push_str(&input[offset..range.start]);
        offset = range.end;
    }
    output.push_str(&input[offset..]);
    output
}

fn top_level_comment_ranges(input: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    let mut fenced = false;
    let mut skip_until = 0;
    for line in input.split_inclusive('\n') {
        let line_end = offset + line.len();
        if offset < skip_until {
            offset = line_end;
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            offset = line_end;
            continue;
        }
        let nested = trimmed.starts_with('>')
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
            || trimmed
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
                && trimmed.contains(". ");
        if !fenced && !nested {
            if let Some(comment_start) = line.find("<!--") {
                if line[..comment_start].trim().is_empty() {
                    let start = offset + comment_start;
                    if let Some(comment_end) = input[start + 4..].find("-->") {
                        let end = start + 4 + comment_end + 3;
                        ranges.push(start..end);
                        skip_until = end;
                    }
                }
            }
        }
        offset = line_end;
    }
    ranges
}

fn is_directive_comment(comment: &str) -> bool {
    comment == "keep-with-next"
        || comment
            .split_once(':')
            .is_some_and(|(name, _)| matches!(name.trim(), "anchor" | "art" | "layout" | "unit"))
}

pub fn normalize(input: &str) -> String {
    input
        .split_whitespace()
        .map(|word| word.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn content_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_rules_in_code_and_quotes() {
        let parsed = parse_document("One\n\n```md\n---\n```\n\n> ---\n\n---\n\nTwo").unwrap();
        assert_eq!(parsed.units.len(), 2);
    }

    #[test]
    fn parses_directives() {
        let parsed =
            parse_document("<!-- anchor: reveal -->\n<!-- layout: full-page -->\nText").unwrap();
        assert_eq!(parsed.units[0].directives.anchor.as_deref(), Some("reveal"));
        assert_eq!(parsed.units[0].word_count, 1);
    }

    #[test]
    fn ignores_directive_comments_in_code() {
        let parsed = parse_document("```html\n<!-- anchor: not-an-anchor -->\n```").unwrap();
        assert!(parsed.units[0].directives.anchor.is_none());
    }
}
