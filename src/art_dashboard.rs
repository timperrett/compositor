use crate::art_brief::{self, ArtBriefInspection};
use crate::assets::{self, AssetRecord, AssetStatus};
use crate::config::Config;
use crate::discovery::discover;
use crate::model::{Severity, ValidationIssue};
use crate::{storage, AppError};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

const DEFAULT_REPORT: &str = "output/reports/art-dashboard.html";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Readiness {
    pub label: String,
    pub default_policy_ready: bool,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DashboardCandidate {
    pub id: String,
    pub file: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DashboardEntry {
    pub art_id: String,
    pub compendium_id: Option<String>,
    pub story_id: Option<String>,
    pub placement: String,
    pub required: bool,
    pub art_brief: String,
    pub registry_status: Option<AssetStatus>,
    pub candidate_count: usize,
    pub candidates: Vec<DashboardCandidate>,
    pub selected_candidate: Option<String>,
    pub selected_file: Option<String>,
    pub approved_artwork: Option<String>,
    pub readiness: Readiness,
    pub diagnostics: Vec<DashboardDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DashboardDiagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Dashboard {
    pub entries: Vec<DashboardEntry>,
    pub required_count: usize,
    pub default_policy_ready_count: usize,
    pub blocker_count: usize,
    pub orphan_count: usize,
}

#[derive(Debug, Clone)]
struct EntryLocation {
    required: bool,
    compendium_id: Option<String>,
    story_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DashboardOutput {
    output: String,
    artwork_count: usize,
    required_count: usize,
    default_policy_ready_count: usize,
    blocker_count: usize,
    orphan_count: usize,
}

impl DashboardOutput {
    pub(crate) fn from_dashboard(root: &Path, output: &Path, dashboard: &Dashboard) -> Self {
        Self {
            output: relative_to_root(root, output),
            artwork_count: dashboard.entries.len(),
            required_count: dashboard.required_count,
            default_policy_ready_count: dashboard.default_policy_ready_count,
            blocker_count: dashboard.blocker_count,
            orphan_count: dashboard.orphan_count,
        }
    }
}

pub(crate) fn resolve_output(root: &Path, output: &Path) -> Result<PathBuf, AppError> {
    if output.is_absolute()
        || output.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(AppError::command(
            "art dashboard output must be a project-relative path".into(),
        ));
    }
    let output = if output.as_os_str().is_empty() {
        PathBuf::from(DEFAULT_REPORT)
    } else {
        output.to_path_buf()
    };
    if output
        .extension()
        .is_none_or(|extension| extension != "html")
    {
        return Err(AppError::command(
            "art dashboard output must use the .html extension".into(),
        ));
    }
    Ok(root.join(output))
}

pub(crate) fn analyze(root: &Path, config: &Config) -> Result<Dashboard, AppError> {
    let project = discover(root, config)?;
    let registry = assets::load(root)?;
    let mut required = BTreeMap::new();
    let mut story_compendiums = BTreeMap::new();
    for story in project
        .compendiums
        .iter()
        .flat_map(|compendium| compendium.stories.iter())
    {
        story_compendiums.insert(story.id.clone(), story.compendium_id.clone());
        for (art_id, requirement) in crate::art::requirements_for_story(root, config, &story.id)? {
            required.insert(art_id, (requirement.story_id, story.compendium_id.clone()));
        }
    }

    let mut ids = required
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    ids.extend(art_brief::ids(root)?);
    if let Some(registry) = registry.as_ref() {
        ids.extend(registry.assets.iter().map(|asset| asset.id.clone()));
    }

    let mut entries = ids
        .into_iter()
        .map(|art_id| {
            let inspection = art_brief::inspect(root, config, &art_id);
            let asset = registry
                .as_ref()
                .and_then(|registry| assets::record(registry, &art_id));
            let required_story = required.get(&art_id).cloned();
            let story_id = required_story
                .as_ref()
                .map(|location| location.0.clone())
                .or_else(|| {
                    inspection
                        .brief
                        .as_ref()
                        .map(|brief| brief.source.story_id.clone())
                });
            let compendium_id = required_story
                .as_ref()
                .map(|location| location.1.clone())
                .or_else(|| {
                    story_id
                        .as_ref()
                        .and_then(|story_id| story_compendiums.get(story_id).cloned())
                });
            build_entry(
                root,
                config,
                art_id,
                EntryLocation {
                    required: required_story.is_some(),
                    compendium_id,
                    story_id,
                },
                asset,
                inspection,
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.story_id
            .cmp(&right.story_id)
            .then_with(|| left.art_id.cmp(&right.art_id))
    });

    let required_count = entries.iter().filter(|entry| entry.required).count();
    let default_policy_ready_count = entries
        .iter()
        .filter(|entry| entry.required && entry.readiness.default_policy_ready)
        .count();
    let blocker_count = required_count.saturating_sub(default_policy_ready_count);
    let orphan_count = entries.iter().filter(|entry| !entry.required).count();
    Ok(Dashboard {
        entries,
        required_count,
        default_policy_ready_count,
        blocker_count,
        orphan_count,
    })
}

pub(crate) fn readiness_for(
    root: &Path,
    _config: &Config,
    art_id: &str,
    _required: bool,
    asset: Option<&AssetRecord>,
    inspection: &ArtBriefInspection,
) -> Readiness {
    let candidate_count = inspection
        .brief
        .as_ref()
        .map(|brief| brief.candidates.len())
        .unwrap_or(0);
    if inspection.brief.is_none() {
        return readiness(
            "missing brief",
            false,
            format!("compositor art inspect {art_id}"),
        );
    }
    if !inspection.validation.can_proceed() {
        return readiness(
            "invalid brief",
            false,
            format!("compositor art inspect {art_id}"),
        );
    }
    let Some(asset) = asset else {
        return readiness(
            "needs registration",
            false,
            format!("compositor art register {art_id}"),
        );
    };
    if !assets::validate_record(root, asset).can_proceed() {
        return readiness(
            "invalid record",
            false,
            format!("compositor art inspect {art_id}"),
        );
    }
    match asset.status {
        AssetStatus::Requested if candidate_count == 0 => readiness(
            "no candidate",
            false,
            format!("compositor art inspect {art_id}"),
        ),
        AssetStatus::Requested => readiness(
            "needs selection",
            false,
            candidate_action(art_id, inspection),
        ),
        AssetStatus::Draft => readiness(
            "selected draft",
            true,
            format!("compositor art review {art_id}"),
        ),
        AssetStatus::Review => readiness(
            "in review",
            true,
            format!("compositor art approve {art_id}"),
        ),
        AssetStatus::Approved => readiness("approved", true, "No action required".into()),
        AssetStatus::Rejected => readiness(
            "rejected",
            false,
            format!("compositor art inspect {art_id}"),
        ),
        AssetStatus::Superseded => readiness(
            "superseded",
            false,
            format!("compositor art inspect {art_id}"),
        ),
    }
}

fn readiness(label: &str, default_policy_ready: bool, next_action: String) -> Readiness {
    Readiness {
        label: label.into(),
        default_policy_ready,
        next_action,
    }
}

fn candidate_action(art_id: &str, inspection: &ArtBriefInspection) -> String {
    let candidates = inspection
        .brief
        .as_ref()
        .map(|brief| &brief.candidates)
        .expect("readiness only calls candidate_action for a valid brief");
    if candidates.len() == 1 {
        format!("compositor art select {art_id} {}", candidates[0].id)
    } else {
        format!("compositor art select {art_id} <candidate-id>")
    }
}

fn build_entry(
    root: &Path,
    config: &Config,
    art_id: String,
    location: EntryLocation,
    asset: Option<&AssetRecord>,
    inspection: ArtBriefInspection,
) -> DashboardEntry {
    let readiness = readiness_for(root, config, &art_id, location.required, asset, &inspection);
    let mut diagnostics = inspection
        .validation
        .issues
        .iter()
        .map(diagnostic)
        .collect::<Vec<_>>();
    if let Some(asset) = asset {
        diagnostics.extend(
            assets::validate_record(root, asset)
                .issues
                .iter()
                .map(diagnostic),
        );
    }
    diagnostics.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then_with(|| left.message.cmp(&right.message))
    });
    diagnostics.dedup_by(|left, right| left.code == right.code && left.message == right.message);

    let candidates: Vec<DashboardCandidate> = inspection
        .brief
        .as_ref()
        .map(|brief| {
            brief
                .candidates
                .iter()
                .map(|candidate| DashboardCandidate {
                    id: candidate.id.clone(),
                    file: candidate.file.clone(),
                    selected: asset
                        .and_then(|asset| asset.selection.as_ref())
                        .is_some_and(|selection| selection.candidate_id == candidate.id),
                })
                .collect()
        })
        .unwrap_or_default();
    let placement = match inspection.brief.as_ref() {
        Some(_) if !location.required => "unplaced/orphan".into(),
        Some(brief) if brief.usage == crate::art_brief::ArtUsage::Opener => "opener".into(),
        Some(brief) if !brief.source.spread_ids.is_empty() => {
            format!("spreads: {}", brief.source.spread_ids.join(", "))
        }
        Some(_) => "story art".into(),
        None if location.required => "planned art".into(),
        None => "unplaced/orphan".into(),
    };
    DashboardEntry {
        art_id,
        compendium_id: location.compendium_id,
        story_id: location.story_id,
        placement,
        required: location.required,
        art_brief: inspection.path,
        registry_status: asset.map(|asset| asset.status),
        candidate_count: candidates.len(),
        candidates,
        selected_candidate: asset
            .and_then(|asset| asset.selection.as_ref())
            .map(|selection| selection.candidate_id.clone()),
        selected_file: asset
            .and_then(|asset| asset.selection.as_ref())
            .map(|selection| selection.file.clone()),
        approved_artwork: asset
            .and_then(|asset| asset.approved.as_ref())
            .map(|approved| approved.file.clone()),
        readiness,
        diagnostics,
    }
}

fn diagnostic(issue: &ValidationIssue) -> DashboardDiagnostic {
    DashboardDiagnostic {
        severity: issue.severity.clone(),
        code: issue.code.clone(),
        message: issue.message.clone(),
    }
}

pub(crate) fn write_html(
    root: &Path,
    output: &Path,
    dashboard: &Dashboard,
) -> Result<(), AppError> {
    storage::write_text_atomic(output, &render_html(root, output, dashboard))
}

fn render_html(root: &Path, output: &Path, dashboard: &Dashboard) -> String {
    let mut html = String::from(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Compositor art dashboard</title><style>:root{color:#1f211e;background:#f6f1e8;font-family:Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;font-synthesis:none}*{box-sizing:border-box}body{min-width:320px;margin:0;background-color:#f6f1e8;background-image:radial-gradient(rgba(80,66,49,.09) .55px,transparent .55px);background-size:7px 7px;color:#1f211e;line-height:1.5;text-rendering:optimizeLegibility}a{color:inherit}a:focus-visible,select:focus-visible,input:focus-visible{outline:3px solid #a34e32;outline-offset:3px}.shell{width:min(100% - 2rem,76rem);margin-inline:auto}.site-header{position:sticky;top:0;z-index:10;border-bottom:1px solid #d7cdbf;background:rgba(246,241,232,.95);backdrop-filter:blur(8px)}.site-header .shell{display:flex;align-items:center;justify-content:space-between;gap:1rem;min-height:4rem}.brand{font-family:"Iowan Old Style","Palatino Linotype","Book Antiqua",Palatino,Georgia,serif;font-size:1.45rem;letter-spacing:-.03em}.product-label{margin:0;color:#4d504a;font-size:.72rem;font-weight:700;letter-spacing:.1em;text-transform:uppercase}.hero{padding:4.5rem 0 2.25rem;max-width:54rem}.eyebrow,.rule-label{display:flex;align-items:center;gap:.7rem;margin:0;color:#58705d;font-size:.7rem;font-weight:700;letter-spacing:.14em;line-height:1.2;text-transform:uppercase}.eyebrow::before{width:1.65rem;height:1px;content:"";background:currentColor}.rule-label::after{flex:1;height:1px;content:"";background:#d7cdbf}.hero h1,.story h2{font-family:"Iowan Old Style","Palatino Linotype","Book Antiqua",Palatino,Georgia,serif;font-weight:600;letter-spacing:-.035em}.hero h1{max-width:46rem;margin:.9rem 0 0;font-size:clamp(2.75rem,7vw,5.25rem);line-height:.98}.lead{max-width:48rem;margin:1.5rem 0 0;color:#4d504a;font-size:1.05rem;line-height:1.8}.summary{display:grid;grid-template-columns:repeat(auto-fit,minmax(11rem,1fr));gap:1px;margin:2rem 0 3rem;overflow:hidden;border:1px solid #d7cdbf;background:#d7cdbf}.stat{min-height:7.25rem;padding:1.25rem;background:rgba(255,252,246,.72)}.stat strong{display:block;color:#a34e32;font-family:"Iowan Old Style",Georgia,serif;font-size:2rem;font-weight:600;line-height:1}.stat span{display:block;margin-top:.55rem;color:#565850;font-size:.75rem;font-weight:700;letter-spacing:.08em;text-transform:uppercase}.filter-panel{padding:1.3rem 1.5rem 1.5rem;border:1px solid #d7cdbf;background:rgba(251,247,239,.65);box-shadow:0 14px 34px rgba(45,38,28,.08)}.filters{display:grid;grid-template-columns:repeat(auto-fit,minmax(10rem,1fr));gap:1rem;margin-top:1.25rem}.filters label{display:grid;gap:.4rem;color:#4d504a;font-size:.7rem;font-weight:700;letter-spacing:.08em;text-transform:uppercase}.filters select{width:100%;padding:.65rem .7rem;border:1px solid #bcae9c;background:#fffaf2;color:#1f211e;font:inherit;letter-spacing:normal;text-transform:none}.checkbox{display:flex!important;grid-template-columns:auto 1fr;align-items:center;gap:.55rem}.checkbox input{width:1rem;height:1rem;accent-color:#a34e32}.records{margin-top:3.5rem}.story{margin:2.5rem 0}.story h2{margin:0 0 1rem;font-size:clamp(1.8rem,4vw,2.7rem);line-height:1.08}.card{margin:.9rem 0;padding:1.3rem;border:1px solid #d7cdbf;border-left:4px solid #7a8076;background:rgba(255,252,246,.82);box-shadow:0 8px 18px rgba(45,38,28,.06)}.card.ready{border-left-color:#58705d}.card.blocked{border-left-color:#a34e32}.card.orphan{border-left-color:#7a6249}.card h3{margin:0;font-family:"Iowan Old Style","Palatino Linotype",Georgia,serif;font-size:1.55rem;line-height:1.1}.badges{display:flex;flex-wrap:wrap;gap:.45rem;margin:.8rem 0}.badge{display:inline-block;padding:.18rem .45rem;border:1px solid #d7cdbf;background:#f6f1e8;color:#4d504a;font-family:"SFMono-Regular",Consolas,"Liberation Mono",monospace;font-size:.68rem;letter-spacing:.04em;text-transform:uppercase}.meta{margin:.35rem 0;color:#565850;font-size:.88rem}.meta a{color:#58705d;text-decoration-color:#a34e32;text-underline-offset:.2rem}.candidates{display:flex;gap:.8rem;flex-wrap:wrap;margin:1rem 0}.candidate{display:block;max-width:180px;padding:.35rem;border:1px solid #d7cdbf;background:#f6f1e8;text-decoration:none}.candidate:hover{border-color:#a34e32}.candidate.selected{border:2px solid #58705d}.candidate img{display:block;width:170px;max-width:100%;height:130px;object-fit:contain;background:#fffaf2}.candidate small{display:block;margin-top:.3rem;color:#565850;font-size:.67rem}code{overflow-wrap:anywhere;font-family:"SFMono-Regular",Consolas,"Liberation Mono",monospace;font-size:.83em}.action{margin:.9rem 0 0;padding:.75rem .85rem;border-left:2px solid #a34e32;background:#f3ebdf}.empty{color:#686961}details{margin-top:1rem;padding-top:.75rem;border-top:1px solid #d7cdbf}summary{cursor:pointer;font-weight:700}details ul{padding-left:1.25rem;color:#4d504a}@media(max-width:640px){.site-header .shell{align-items:flex-start;flex-direction:column;justify-content:center;padding-block:.65rem}.hero{padding-top:3rem}.filter-panel{padding:1rem}.card{padding:1rem}}</style></head><body><header class="site-header"><div class="shell"><div class="brand">Compositor</div><p class="product-label">Art dashboard · local report</p></div></header><main class="shell"><section class="hero"><p class="eyebrow">Artwork lifecycle</p><h1>Make the next art decision legible.</h1><p class="lead">Read-only view of current on-disk artwork state. Default package policy: selected <code>draft</code>, <code>review</code>, or <code>approved</code> artwork.</p></section>"#,
    );
    html.push_str(&format!("<section class=\"summary\"><div class=\"stat\"><strong>{}</strong><span>art records</span></div><div class=\"stat\"><strong>{} / {}</strong><span>required art meets draft policy</span></div><div class=\"stat\"><strong>{}</strong><span>required blockers</span></div><div class=\"stat\"><strong>{}</strong><span>unplaced/orphan records</span></div></section>", dashboard.entries.len(), dashboard.default_policy_ready_count, dashboard.required_count, dashboard.blocker_count, dashboard.orphan_count));
    html.push_str("<section class=\"filter-panel\"><p class=\"rule-label\">Filter artwork</p><div class=\"filters\"><label>Compendium<select id=\"compendium\"><option value=\"\">All compendiums</option>");
    let compendiums = dashboard
        .entries
        .iter()
        .filter_map(|entry| entry.compendium_id.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    for compendium in compendiums {
        html.push_str(&format!(
            "<option value=\"{}\">{}</option>",
            escape(compendium),
            escape(compendium)
        ));
    }
    html.push_str(
        "</select></label><label>Story<select id=\"story\"><option value=\"\">All stories</option>",
    );
    let stories = dashboard
        .entries
        .iter()
        .filter_map(|entry| entry.story_id.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    for story in stories {
        html.push_str(&format!(
            "<option value=\"{}\">{}</option>",
            escape(story),
            escape(story)
        ));
    }
    html.push_str("</select></label><label>Lifecycle<select id=\"lifecycle\"><option value=\"\">All lifecycle states</option><option value=\"unregistered\">Unregistered</option><option value=\"requested\">Requested</option><option value=\"draft\">Draft</option><option value=\"review\">Review</option><option value=\"approved\">Approved</option><option value=\"rejected\">Rejected</option><option value=\"superseded\">Superseded</option></select></label><label>Readiness<select id=\"readiness\"><option value=\"\">All readiness</option>");
    let readiness = dashboard
        .entries
        .iter()
        .map(|entry| entry.readiness.label.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for label in readiness {
        html.push_str(&format!(
            "<option value=\"{}\">{}</option>",
            escape(label),
            escape(label)
        ));
    }
    html.push_str("</select></label><label class=\"checkbox\"><input id=\"blockers\" type=\"checkbox\"> Required blockers only</label></div></section><section class=\"records\"><p class=\"rule-label\">Artwork records</p>");

    let mut by_story = BTreeMap::<String, Vec<&DashboardEntry>>::new();
    for entry in &dashboard.entries {
        by_story
            .entry(
                entry
                    .story_id
                    .clone()
                    .unwrap_or_else(|| "Unplaced/orphan".into()),
            )
            .or_default()
            .push(entry);
    }
    for (story, entries) in by_story {
        html.push_str(&format!(
            "<section class=\"story\"><h2>{}</h2>",
            escape(&story)
        ));
        for entry in entries {
            render_entry(&mut html, root, output, entry);
        }
        html.push_str("</section>");
    }
    html.push_str("</section></main><script>const controls=Object.fromEntries(['compendium','story','lifecycle','readiness','blockers'].map(id=>[id,document.getElementById(id)]));function filter(){const compendium=controls.compendium.value,story=controls.story.value,state=controls.lifecycle.value,ready=controls.readiness.value,blockers=controls.blockers.checked;document.querySelectorAll('.card').forEach(card=>{const show=(!compendium||card.dataset.compendium===compendium)&&(!story||card.dataset.story===story)&&(!state||card.dataset.lifecycle===state)&&(!ready||card.dataset.readiness===ready)&&(!blockers||card.dataset.blocker==='true');card.hidden=!show});document.querySelectorAll('.story').forEach(section=>section.hidden=!Array.from(section.querySelectorAll('.card')).some(card=>!card.hidden))}Object.values(controls).forEach(control=>control.addEventListener('change',filter));</script></body></html>\n");
    html
}

fn render_entry(html: &mut String, root: &Path, output: &Path, entry: &DashboardEntry) {
    let lifecycle = entry
        .registry_status
        .map(status_name)
        .unwrap_or("unregistered");
    let class = if !entry.required {
        "orphan"
    } else if entry.readiness.default_policy_ready {
        "ready"
    } else {
        "blocked"
    };
    let story = entry.story_id.as_deref().unwrap_or("Unplaced/orphan");
    let compendium = entry.compendium_id.as_deref().unwrap_or("Unplaced/orphan");
    html.push_str(&format!("<article class=\"card {}\" data-compendium=\"{}\" data-story=\"{}\" data-lifecycle=\"{}\" data-readiness=\"{}\" data-blocker=\"{}\"><h3>{}</h3><p class=\"badges\"><span class=\"badge\">{}</span><span class=\"badge\">{}</span><span class=\"badge\">{}</span></p><p class=\"meta\">Compendium: {} · placement: {} · candidates: {} · brief: <a href=\"{}\">{}</a></p>", class, escape(compendium), escape(story), lifecycle, escape(&entry.readiness.label), entry.required && !entry.readiness.default_policy_ready, escape(&entry.art_id), escape(lifecycle), escape(&entry.readiness.label), if entry.required { "required" } else { "unplaced/orphan" }, escape(compendium), escape(&entry.placement), entry.candidate_count, escape(&report_link(root, output, &entry.art_brief)), escape(&entry.art_brief)));
    if let Some(file) = entry.selected_file.as_deref() {
        html.push_str(&format!(
            "<p class=\"meta\">Selected: <code>{}</code></p>",
            escape(file)
        ));
    }
    if let Some(file) = entry.approved_artwork.as_deref() {
        html.push_str(&format!(
            "<p class=\"meta\">Approved: <code>{}</code></p>",
            escape(file)
        ));
    }
    if entry.candidates.is_empty() {
        html.push_str("<p class=\"empty\">No candidate files are registered in this brief.</p>");
    } else {
        html.push_str("<div class=\"candidates\">");
        for candidate in &entry.candidates {
            let selected = if candidate.selected { " selected" } else { "" };
            let link = report_link(root, output, &candidate.file);
            html.push_str(&format!("<a class=\"candidate{}\" href=\"{}\"><img src=\"{}\" alt=\"Candidate {} for {}\"><code>{}</code><br><small>{}</small></a>", selected, escape(&link), escape(&link), escape(&candidate.id), escape(&entry.art_id), escape(&candidate.id), escape(&candidate.file)));
        }
        html.push_str("</div>");
    }
    html.push_str(&format!(
        "<p class=\"action\"><strong>Next action:</strong> <code>{}</code></p>",
        escape(&entry.readiness.next_action)
    ));
    if !entry.diagnostics.is_empty() {
        html.push_str("<details><summary>Diagnostics</summary><ul>");
        for diagnostic in &entry.diagnostics {
            html.push_str(&format!(
                "<li><strong>{}</strong> [{}] {}</li>",
                escape(&format!("{:?}", diagnostic.severity).to_lowercase()),
                escape(&diagnostic.code),
                escape(&diagnostic.message)
            ));
        }
        html.push_str("</ul></details>");
    }
    html.push_str("</article>");
}

fn report_link(root: &Path, output: &Path, file: &str) -> String {
    let from = output.parent().unwrap_or(root);
    relative_between(from, &root.join(file))
}

fn relative_between(from: &Path, to: &Path) -> String {
    let from = normal_components(from);
    let to = normal_components(to);
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts = vec![".."; from.len().saturating_sub(common)];
    parts.extend(to[common..].iter().map(String::as_str));
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

fn normal_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect()
}

fn relative_to_root(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn status_name(status: AssetStatus) -> &'static str {
    match status {
        AssetStatus::Requested => "requested",
        AssetStatus::Draft => "draft",
        AssetStatus::Review => "review",
        AssetStatus::Approved => "approved",
        AssetStatus::Rejected => "rejected",
        AssetStatus::Superseded => "superseded",
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_authored_html_text() {
        assert_eq!(escape("<&>\"'"), "&lt;&amp;&gt;&quot;&#39;");
    }

    #[test]
    fn computes_links_relative_to_the_report_location() {
        assert_eq!(
            relative_between(
                Path::new("/project/output/reports"),
                Path::new("/project/assets/drafts/art/a.png")
            ),
            "../../assets/drafts/art/a.png"
        );
    }
}
