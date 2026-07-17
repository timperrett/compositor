use crate::model::{ManifestStory, PageFragment, PagePlan, Story};

pub fn render_html(story: &Story, plan: &PagePlan, manifest_story: &ManifestStory) -> String {
    let mut output = format!("<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title><style>body{{font-family:serif;max-width:42rem;margin:3rem auto}}.page{{border-top:1px solid #aaa;padding:1rem 0}}.meta{{color:#666;font-family:monospace}}</style></head><body><h1>{}</h1><p class=\"meta\">Plan v{} / manifest v{}</p>", escape(&story.title), escape(&story.title), plan.revision, plan.manifest_revision);
    for assignment in &plan.assignments {
        let content = if assignment.fragments.is_empty() {
            assignment
                .units
                .iter()
                .map(|unit_id| render_whole_unit(story, manifest_story, unit_id))
                .collect::<String>()
        } else {
            assignment
                .fragments
                .iter()
                .map(|fragment| render_fragment(story, manifest_story, fragment))
                .collect::<String>()
        };
        output.push_str(&format!("<section class=\"page\"><p class=\"meta\">pages {:?} · {} · {}</p><article>{}</article>", assignment.pages, escape(&assignment.units.join(", ")), escape(&assignment.layout), content));
        if assignment.layout.contains("art")
            || assignment.layout == "full-page"
            || assignment.layout == "full-spread"
        {
            output.push_str("<p class=\"meta\">[missing artwork]</p>");
        }
        output.push_str("</section>");
    }
    if !plan.warnings.is_empty() {
        output.push_str("<h2>Warnings</h2><ul>");
        for warning in &plan.warnings {
            output.push_str(&format!("<li>{}</li>", escape(warning)));
        }
        output.push_str("</ul>");
    }
    output.push_str("</body></html>\n");
    output
}

fn render_whole_unit(story: &Story, manifest_story: &ManifestStory, unit_id: &str) -> String {
    find_unit_content(story, manifest_story, unit_id)
        .map(markdown_to_html)
        .unwrap_or_else(|| "<p>[source unit unavailable]</p>".into())
}

fn render_fragment(
    story: &Story,
    manifest_story: &ManifestStory,
    fragment: &PageFragment,
) -> String {
    find_unit_content(story, manifest_story, &fragment.unit_id)
        .map(|content| {
            markdown_to_html(&word_range(content, fragment.start_word, fragment.end_word))
        })
        .unwrap_or_else(|| "<p>[source unit unavailable]</p>".into())
}

fn find_unit_content<'a>(
    story: &'a Story,
    manifest_story: &ManifestStory,
    unit_id: &str,
) -> Option<&'a str> {
    manifest_story
        .units
        .iter()
        .find(|unit| unit.id == unit_id)
        .and_then(|manifest_unit| {
            story
                .units
                .iter()
                .find(|unit| unit.ordinal == manifest_unit.ordinal)
        })
        .map(|unit| unit.content.as_str())
}

fn word_range(value: &str, start_word: usize, end_word: usize) -> String {
    let visible = crate::markdown::strip_directives(value);
    let (mut ranges, trailing_start) = visible.char_indices().fold(
        (Vec::new(), None),
        |(mut ranges, start), (index, character)| match (start, character.is_whitespace()) {
            (None, false) => (ranges, Some(index)),
            (Some(start), true) => {
                ranges.push((start, index));
                (ranges, None)
            }
            _ => (ranges, start),
        },
    );
    if let Some(start) = trailing_start {
        ranges.push((start, visible.len()));
    }
    let Some((start, _)) = ranges.get(start_word) else {
        return String::new();
    };
    let Some((_, end)) = ranges.get(end_word.saturating_sub(1)) else {
        return String::new();
    };
    visible[*start..*end].trim().into()
}

fn markdown_to_html(value: &str) -> String {
    format!(
        "<p>{}</p>",
        escape(&crate::markdown::strip_directives(value))
            .replace("\n\n", "</p><p>")
            .replace('\n', "<br>")
    )
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::word_range;

    #[test]
    fn extracts_the_requested_word_range_without_losing_internal_whitespace() {
        assert_eq!(word_range("one two\nthree four", 1, 3), "two\nthree");
    }
}
