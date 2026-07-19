use crate::markdown::valid_anchor;
use crate::model::{Severity, ValidationIssue, ValidationReport};
use crate::{storage, AppError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const ASSET_REGISTRY_SCHEMA: &str = "compositor.dev/art-assets/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssetRegistry {
    pub schema: String,
    #[serde(default)]
    pub assets: Vec<AssetRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssetRecord {
    pub id: String,
    pub brief: String,
    pub status: AssetStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum AssetStatus {
    Requested,
    Draft,
    Review,
    Approved,
    Rejected,
    Superseded,
}

impl AssetStatus {
    pub const fn placeable(self) -> bool {
        matches!(self, Self::Draft | Self::Review | Self::Approved)
    }

    pub const fn rank(self) -> Option<u8> {
        match self {
            Self::Draft => Some(1),
            Self::Review => Some(2),
            Self::Approved => Some(3),
            Self::Requested | Self::Rejected | Self::Superseded => None,
        }
    }
}

pub fn path(root: &Path) -> PathBuf {
    root.join("art/assets.yaml")
}

pub fn load(root: &Path) -> Result<Option<AssetRegistry>, AppError> {
    load_from(&path(root))
}

pub fn load_from(path: &Path) -> Result<Option<AssetRegistry>, AppError> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    serde_yaml::from_str(&text)
        .map(Some)
        .map_err(|error| AppError::serialization(format!("{}: {error}", path.display())))
}

pub fn save(root: &Path, registry: &AssetRegistry) -> Result<(), AppError> {
    let path = path(root);
    let text = serde_yaml::to_string(registry)
        .map_err(|error| AppError::serialization(error.to_string()))?;
    storage::write_text_atomic(&path, &text)
}

pub fn record<'a>(registry: &'a AssetRegistry, id: &str) -> Option<&'a AssetRecord> {
    registry.assets.iter().find(|asset| asset.id == id)
}

pub fn record_mut<'a>(registry: &'a mut AssetRegistry, id: &str) -> Option<&'a mut AssetRecord> {
    registry.assets.iter_mut().find(|asset| asset.id == id)
}

pub fn validate(root: &Path, registry: &AssetRegistry) -> ValidationReport {
    let mut report = ValidationReport::default();
    let registry_path = relative(root, &path(root));
    if registry.schema != ASSET_REGISTRY_SCHEMA {
        issue(
            &mut report,
            "ART_REGISTRY_SCHEMA_UNSUPPORTED",
            "unsupported asset registry schema",
            &registry_path,
            None,
        );
    }
    let mut ids = BTreeSet::new();
    for asset in &registry.assets {
        if !valid_anchor(&asset.id) || !ids.insert(&asset.id) {
            issue(
                &mut report,
                "ART_ASSET_ID_INVALID",
                "asset IDs must be unique lowercase kebab-case",
                &registry_path,
                Some(&asset.id),
            );
        }
        if asset.brief != format!("art/briefs/{}.yaml", asset.id) {
            issue(
                &mut report,
                "ART_BRIEF_LINK_INVALID",
                "each v1 asset must link its matching brief path",
                &registry_path,
                Some(&asset.id),
            );
        }
        if !root.join(&asset.brief).is_file() {
            issue(
                &mut report,
                "ART_BRIEF_MISSING",
                "linked art brief does not exist",
                &registry_path,
                Some(&asset.id),
            );
        }
        match asset.status {
            AssetStatus::Requested => {
                if asset.file.is_some() {
                    issue(
                        &mut report,
                        "ART_REQUESTED_HAS_FILE",
                        "requested assets must not have a placeable file",
                        &registry_path,
                        Some(&asset.id),
                    );
                }
            }
            AssetStatus::Draft | AssetStatus::Review | AssetStatus::Approved => {
                let Some(file) = asset.file.as_deref() else {
                    issue(
                        &mut report,
                        "ART_FILE_MISSING",
                        "placeable asset statuses require a file",
                        &registry_path,
                        Some(&asset.id),
                    );
                    continue;
                };
                validate_file(root, file, &registry_path, &asset.id, &mut report);
                if asset.status == AssetStatus::Approved && !file.starts_with("assets/approved/") {
                    issue(
                        &mut report,
                        "ART_APPROVED_PATH_INVALID",
                        "approved assets must live under assets/approved",
                        &registry_path,
                        Some(&asset.id),
                    );
                }
            }
            AssetStatus::Rejected => {}
            AssetStatus::Superseded => {
                let Some(successor) = asset.superseded_by.as_deref() else {
                    issue(
                        &mut report,
                        "ART_SUPERSEDED_BY_MISSING",
                        "superseded assets require superseded_by",
                        &registry_path,
                        Some(&asset.id),
                    );
                    continue;
                };
                if successor == asset.id
                    || !registry
                        .assets
                        .iter()
                        .any(|candidate| candidate.id == successor)
                {
                    issue(
                        &mut report,
                        "ART_SUPERSEDED_BY_UNKNOWN",
                        "superseded_by must name another registry asset",
                        &registry_path,
                        Some(&asset.id),
                    );
                }
            }
        }
    }
    report
}

pub fn allowed(status: AssetStatus, policy: AssetStatus) -> bool {
    match (status.rank(), policy.rank()) {
        (Some(actual), Some(required)) => actual >= required,
        _ => false,
    }
}

pub fn transition(
    record: &mut AssetRecord,
    next: AssetStatus,
    file: Option<String>,
) -> Result<(), AppError> {
    let allowed = matches!(
        (record.status, next),
        (AssetStatus::Requested, AssetStatus::Draft)
            | (
                AssetStatus::Draft,
                AssetStatus::Draft | AssetStatus::Review | AssetStatus::Rejected
            )
            | (
                AssetStatus::Review,
                AssetStatus::Draft | AssetStatus::Approved | AssetStatus::Rejected
            )
            | (AssetStatus::Approved, AssetStatus::Superseded)
    );
    if !allowed {
        return Err(AppError::command(format!(
            "invalid asset transition from {:?} to {:?}",
            record.status, next
        )));
    }
    record.status = next;
    if file.is_some() {
        record.file = file;
    }
    Ok(())
}

fn validate_file(
    root: &Path,
    value: &str,
    registry_path: &str,
    id: &str,
    report: &mut ValidationReport,
) {
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        issue(
            report,
            "ART_PATH_UNSAFE",
            "asset files must be safe project-relative paths",
            registry_path,
            Some(id),
        );
        return;
    }
    if !root.join(path).is_file() {
        issue(
            report,
            "ART_FILE_MISSING",
            "asset file does not exist",
            registry_path,
            Some(id),
        );
    }
}

fn issue(
    report: &mut ValidationReport,
    code: &str,
    message: &str,
    path: &str,
    asset: Option<&str>,
) {
    report.issues.push(ValidationIssue {
        severity: Severity::Error,
        code: code.into(),
        message: message.into(),
        path: path.into(),
        story_id: None,
        unit_id: asset.map(str::to_owned),
    });
}

fn relative(root: &Path, value: &Path) -> String {
    value
        .strip_prefix(root)
        .unwrap_or(value)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_order_and_transitions_are_strict() {
        assert!(allowed(AssetStatus::Approved, AssetStatus::Review));
        assert!(!allowed(AssetStatus::Draft, AssetStatus::Review));
        let mut asset = AssetRecord {
            id: "opening-rain".into(),
            brief: "art/briefs/opening-rain.yaml".into(),
            status: AssetStatus::Requested,
            file: None,
            superseded_by: None,
        };
        transition(
            &mut asset,
            AssetStatus::Draft,
            Some("assets/drafts/opening-rain/a.png".into()),
        )
        .unwrap();
        assert!(transition(&mut asset, AssetStatus::Approved, None).is_err());
    }
}
