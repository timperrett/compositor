use crate::model::{Severity, SourceProject, UnitType, ValidationIssue, ValidationReport};
use std::collections::BTreeSet;

pub fn validate(project: &SourceProject) -> ValidationReport {
    let mut report = ValidationReport::default();
    let mut story_ids = BTreeSet::new();
    let mut anchors = BTreeSet::new();
    for compendium in &project.compendiums {
        for story in &compendium.stories {
            if !story_ids.insert(story.id.clone()) {
                issue(
                    &mut report,
                    Severity::Error,
                    "duplicate_story_id",
                    format!("duplicate story ID `{}`", story.id),
                    &story.source,
                    Some(&story.id),
                    None,
                );
            }
            for unit in &story.units {
                let is_blank = unit.directives.unit_type == Some(UnitType::Blank);
                if unit.normalized_content.is_empty() && !is_blank {
                    issue(
                        &mut report,
                        Severity::Error,
                        "empty_unit",
                        "empty content unit; use `<!-- unit: blank -->` for an intentional blank"
                            .into(),
                        &story.source,
                        Some(&story.id),
                        None,
                    );
                }
                if let Some(anchor) = &unit.directives.anchor {
                    if !anchors.insert(anchor.clone()) {
                        issue(
                            &mut report,
                            Severity::Error,
                            "duplicate_anchor",
                            format!("duplicate anchor `{anchor}`"),
                            &story.source,
                            Some(&story.id),
                            Some(anchor),
                        );
                    }
                }
            }
        }
    }
    report
}

fn issue(
    report: &mut ValidationReport,
    severity: Severity,
    code: &str,
    message: String,
    path: &str,
    story_id: Option<&str>,
    unit_id: Option<&str>,
) {
    report.issues.push(ValidationIssue {
        severity,
        code: code.into(),
        message,
        path: path.into(),
        story_id: story_id.map(str::to_owned),
        unit_id: unit_id.map(str::to_owned),
    });
}
