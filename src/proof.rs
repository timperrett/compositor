use crate::model::{ManifestStory, PagePlan, Story};

pub fn render_html(story: &Story, plan: &PagePlan, manifest_story: &ManifestStory) -> String {
    let mut output = format!("<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title><style>body{{font-family:serif;max-width:42rem;margin:3rem auto}}.page{{border-top:1px solid #aaa;padding:1rem 0}}.meta{{color:#666;font-family:monospace}}</style></head><body><h1>{}</h1><p class=\"meta\">Plan v{} / manifest v{}</p>", escape(&story.title), escape(&story.title), plan.revision, plan.manifest_revision);
    for assignment in &plan.assignments {
        let content = assignment
            .units
            .iter()
            .map(|unit_id| {
                manifest_story
                    .units
                    .iter()
                    .find(|unit| unit.id == *unit_id)
                    .and_then(|manifest_unit| {
                        story
                            .units
                            .iter()
                            .find(|unit| unit.ordinal == manifest_unit.ordinal)
                    })
                    .map(|unit| markdown_to_html(&unit.content))
                    .unwrap_or_else(|| "<p>[source unit unavailable]</p>".into())
            })
            .collect::<String>();
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
