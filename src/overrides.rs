use crate::composition::{CompositionPlan, CompositionSpread};
use crate::model::{Severity, ValidationIssue, ValidationReport};
use crate::{storage, AppError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const OVERRIDES_SCHEMA: &str = "compositor.dev/composition-overrides/v1";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompositionOverrides {
    pub schema: String,
    #[serde(default)]
    pub spreads: BTreeMap<String, SpreadOverride>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpreadOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<LayoutOverride>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_density: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art_assets: Option<Vec<crate::composition::ArtReference>>,
    #[serde(default)]
    pub locks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LayoutOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedComposition {
    pub plan: CompositionPlan,
    pub provenance: BTreeMap<String, String>,
}

pub fn load(path: &Path) -> Result<CompositionOverrides, AppError> {
    let text = fs::read_to_string(path)?;
    serde_yaml::from_str(&text)
        .map_err(|error| AppError::serialization(format!("{}: {error}", path.display())))
}

pub fn reconcile(
    plan: &CompositionPlan,
    overrides: &CompositionOverrides,
) -> (ResolvedComposition, ValidationReport) {
    let mut resolved = plan.clone();
    let mut report = ValidationReport::default();
    let mut provenance = BTreeMap::new();
    if overrides.schema != OVERRIDES_SCHEMA {
        report.issues.push(issue(
            "OVERRIDE_SCHEMA_UNSUPPORTED",
            "unsupported override schema",
            None,
        ));
    }
    for spread in &mut resolved.spreads {
        provenance.insert(
            format!("{}.layout.family", spread.id),
            "composition-plan".into(),
        );
        provenance.insert(
            format!("{}.layout.variant", spread.id),
            "composition-plan".into(),
        );
        provenance.insert(
            format!("{}.text.density", spread.id),
            "composition-plan".into(),
        );
        if let Some(override_value) = overrides.spreads.get(&spread.id) {
            apply(spread, override_value, &mut provenance);
        }
    }
    for id in overrides.spreads.keys() {
        if !resolved.spreads.iter().any(|spread| &spread.id == id) {
            report.issues.push(issue(
                "OVERRIDE_SPREAD_UNKNOWN",
                "override targets an unknown spread",
                Some(id),
            ));
        }
    }
    (
        ResolvedComposition {
            plan: resolved,
            provenance,
        },
        report,
    )
}

fn apply(
    spread: &mut CompositionSpread,
    value: &SpreadOverride,
    provenance: &mut BTreeMap<String, String>,
) {
    if let Some(layout) = &value.layout {
        if let Some(family) = &layout.family {
            spread.layout.family = family.clone();
            provenance.insert(
                format!("{}.layout.family", spread.id),
                "human-override".into(),
            );
        }
        if let Some(variant) = &layout.variant {
            spread.layout.variant = variant.clone();
            provenance.insert(
                format!("{}.layout.variant", spread.id),
                "human-override".into(),
            );
        }
    }
    if let Some(density) = &value.text_density {
        spread.text.density = density.clone();
        provenance.insert(
            format!("{}.text.density", spread.id),
            "human-override".into(),
        );
    }
    if let Some(quiet_region) = &value.quiet_region {
        spread.illustration.quiet_region = Some(quiet_region.clone());
        provenance.insert(
            format!("{}.illustration.quiet_region", spread.id),
            "human-override".into(),
        );
    }
    if let Some(art_assets) = &value.art_assets {
        spread.art_assets = art_assets.clone();
        provenance.insert(format!("{}.art_assets", spread.id), "human-override".into());
    }
}

pub fn write(path: &Path, value: &ResolvedComposition) -> Result<(), AppError> {
    let yaml =
        serde_yaml::to_string(value).map_err(|error| AppError::serialization(error.to_string()))?;
    storage::write_text_atomic(path, &yaml)
}

fn issue(code: &str, message: &str, spread: Option<&str>) -> ValidationIssue {
    ValidationIssue {
        severity: Severity::Error,
        code: code.into(),
        message: message.into(),
        path: "overrides".into(),
        story_id: None,
        unit_id: spread.map(str::to_owned),
    }
}
