use crate::config::Config;
use crate::markdown::strip_directives;
use crate::model::{SourceProject, Story};
use crate::storage;
use crate::AppError;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::path::{Path, PathBuf};

/// Returns the deterministic set of layout-text artifacts for a project.
pub fn export_paths(root: &Path, config: &Config, project: &SourceProject) -> Vec<PathBuf> {
    let directory = config.output_dir(root).join("text");
    project
        .compendiums
        .iter()
        .flat_map(|compendium| {
            compendium
                .stories
                .iter()
                .map(|story| directory.join(format!("{}.txt", story.id)))
                .chain(std::iter::once(
                    directory.join(format!("{}.txt", compendium.id)),
                ))
        })
        .collect()
}

/// Regenerates the plain-text layout exports without changing authored source.
pub fn write_exports(
    root: &Path,
    config: &Config,
    project: &SourceProject,
) -> Result<Vec<PathBuf>, AppError> {
    let directory = config.output_dir(root).join("text");
    let mut paths = Vec::new();
    for compendium in &project.compendiums {
        for story in &compendium.stories {
            let path = directory.join(format!("{}.txt", story.id));
            storage::write_text_if_changed(&path, &render_story(story))?;
            paths.push(path);
        }
        let path = directory.join(format!("{}.txt", compendium.id));
        storage::write_text_if_changed(&path, &render_compendium(&compendium.stories))?;
        paths.push(path);
    }
    Ok(paths)
}

pub fn render_story(story: &Story) -> String {
    let body = story
        .units
        .iter()
        .map(|unit| plain_text(&strip_directives(&unit.content)))
        .filter(|unit| !unit.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if body.is_empty() {
        String::new()
    } else {
        format!("{body}\n")
    }
}

fn render_compendium(stories: &[Story]) -> String {
    stories
        .iter()
        .map(render_story)
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Renders Markdown to text intended for import into a layout application.
pub fn plain_text(input: &str) -> String {
    let mut output = String::new();
    let mut list_depth = 0usize;
    for event in Parser::new_ext(input, Options::all()) {
        match event {
            Event::Start(Tag::Paragraph)
            | Event::Start(Tag::Heading { .. })
            | Event::Start(Tag::BlockQuote(_))
            | Event::Start(Tag::CodeBlock(_))
            | Event::Start(Tag::FootnoteDefinition(_)) => paragraph_break(&mut output),
            Event::Start(Tag::List(_)) => {
                paragraph_break(&mut output);
                list_depth += 1;
            }
            Event::Start(Tag::Item) => {
                line_break(&mut output);
                output.push_str(&"  ".repeat(list_depth.saturating_sub(1)));
                output.push_str("• ");
            }
            Event::End(TagEnd::Paragraph)
            | Event::End(TagEnd::Heading(_))
            | Event::End(TagEnd::BlockQuote(_))
            | Event::End(TagEnd::CodeBlock)
            | Event::End(TagEnd::FootnoteDefinition) => paragraph_break(&mut output),
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                paragraph_break(&mut output);
            }
            Event::End(TagEnd::Item) => line_break(&mut output),
            Event::End(TagEnd::TableCell) => output.push('\t'),
            Event::End(TagEnd::TableRow) => line_break(&mut output),
            Event::Text(value) | Event::Code(value) => output.push_str(&value),
            Event::SoftBreak | Event::HardBreak => line_break(&mut output),
            Event::TaskListMarker(checked) => {
                output.push_str(if checked { "☑ " } else { "☐ " })
            }
            Event::FootnoteReference(label) => {
                output.push_str("Footnote: ");
                output.push_str(&label);
            }
            // Formatting tags, destinations, HTML, and Markdown delimiters are
            // intentionally omitted; their readable text is emitted separately.
            _ => {}
        }
    }
    normalize_spacing(&output)
}

fn line_break(output: &mut String) {
    trim_trailing_spaces(output);
    if !output.ends_with('\n') && !output.is_empty() {
        output.push('\n');
    }
}

fn paragraph_break(output: &mut String) {
    trim_trailing_spaces(output);
    if output.is_empty() {
        return;
    }
    while output.ends_with('\n') {
        output.pop();
    }
    output.push_str("\n\n");
}

fn trim_trailing_spaces(output: &mut String) {
    let trimmed_length = output.trim_end_matches([' ', '\t']).len();
    output.truncate(trimmed_length);
}

fn normalize_spacing(output: &str) -> String {
    output
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::plain_text;

    #[test]
    fn renders_markdown_as_readable_plain_text() {
        let markdown = "<!-- anchor: opening -->\n# A *bold* start\n\nA [linked](https://example.com) **phrase**.\n\n- First item\n- Second item\n\n> A quotation.\n\n```text\ncode line\n```";

        assert_eq!(
            plain_text(markdown),
            "A bold start\n\nA linked phrase.\n\n• First item\n• Second item\n\nA quotation.\n\ncode line"
        );
    }
}
