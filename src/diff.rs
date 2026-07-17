//! Source diffs are produced by the identity resolver and exposed by `compositor diff source`.
//! Plan diffs are deliberately structural: they show page/assignment changes
//! without claiming an aesthetic judgment about the resulting layout.

use crate::model::PagePlan;
pub use crate::model::{Change, ChangeKind, ChangeSet};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize)]
pub struct PlanDiff {
    pub story_id: String,
    pub before_revision: u64,
    pub after_revision: u64,
    pub unchanged: Vec<String>,
    pub removed: Vec<String>,
    pub added: Vec<String>,
}

pub fn compare_plans(before: &PagePlan, after: &PagePlan) -> PlanDiff {
    let before_items = before
        .assignments
        .iter()
        .map(assignment_label)
        .collect::<BTreeSet<_>>();
    let after_items = after
        .assignments
        .iter()
        .map(assignment_label)
        .collect::<BTreeSet<_>>();
    PlanDiff {
        story_id: after.story_id.clone(),
        before_revision: before.revision,
        after_revision: after.revision,
        unchanged: before_items.intersection(&after_items).cloned().collect(),
        removed: before_items.difference(&after_items).cloned().collect(),
        added: after_items.difference(&before_items).cloned().collect(),
    }
}

pub fn render_plan_diff_html(diff: &PlanDiff) -> String {
    fn list(title: &str, values: &[String]) -> String {
        let items = values
            .iter()
            .map(|value| format!("<li>{}</li>", escape(value)))
            .collect::<String>();
        format!("<h2>{title}</h2><ul>{items}</ul>")
    }
    format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Plan diff</title><style>body{{font-family:system-ui;max-width:55rem;margin:3rem auto}}li{{margin:.4rem 0}}</style><h1>{}</h1><p>v{:03} → v{:03}</p>{}{}{}",
        escape(&diff.story_id), diff.before_revision, diff.after_revision,
        list("Unchanged assignments", &diff.unchanged),
        list("Removed assignments", &diff.removed),
        list("Added assignments", &diff.added),
    )
}

fn assignment_label(assignment: &crate::model::PageAssignment) -> String {
    format!(
        "pages {:?} · {} · {}",
        assignment.pages,
        assignment.units.join(", "),
        assignment.layout
    )
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
