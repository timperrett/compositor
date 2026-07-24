use compositor::config::DEFAULT_CONFIG;
use compositor::migration::{self, MigrationOptions};
use std::fs;
use std::process::Command;

fn package_project() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("compositor.toml"), DEFAULT_CONFIG).unwrap();
    let compendium = directory.path().join("compendiums/01-magic");
    let story_directory = compendium.join("01-story");
    fs::create_dir_all(&story_directory).unwrap();
    fs::write(
        compendium.join("index.md"),
        "---\nid: magic\ntitle: Magic\n---\nA collection.\n",
    )
    .unwrap();
    let story_path = story_directory.join("story.md");
    fs::write(
        &story_path,
        "---\nid: story\ntitle: Story\n---\n<!-- anchor: opening -->\n<!-- paragraph: opening -->\n\nOnce upon a time.\n",
    )
    .unwrap();
    let story = compositor::flow::load_story(&story_path).unwrap();
    fs::write(
        story_directory.join("story.flow.yaml"),
        format!(
            "schema: compositor.dev/story-flow/v1\nstory:\n  id: story\n  source_revision: {}\nspreads:\n  - id: spread-001\n    source:\n      from: {{ type: paragraph, id: opening }}\n      through: {{ type: paragraph, id: opening }}\n    role: opening\n    energy: 1\n    narrative: {{ purpose: Open the story. }}\n",
            story.source_hash
        ),
    )
    .unwrap();
    fs::write(
        story_directory.join("hardcover.composition.yaml"),
        "schema: compositor.dev/composition-plan/v2\nstory:\n  id: story\n  flow: story.flow.yaml\nedition:\n  id: hardcover\n  design_system: edgar-v1\nopener:\n  title: Story\n  placement: center-page\n  art: { id: opener-art, role: primary-subject }\nspreads:\n  - id: spread-001\n    layout: { family: text, variant: standard }\n    text: { density: standard }\n    illustration: { mode: none, focal_subject: none }\n",
    )
    .unwrap();
    fs::create_dir_all(directory.path().join("art/briefs")).unwrap();
    fs::write(
        directory.path().join("art/briefs/opener-art.yaml"),
        "schema_version: 3\nart_id: opener-art\nsource:\n  story_id: story\n  anchor_id: opening\nusage: opener\ngeneration:\n  page_treatment: floating\n  prompt: A test opener.\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("art/assets.yaml"),
        "schema: compositor.dev/art-assets/v2\nassets:\n  - id: opener-art\n    brief: art/briefs/opener-art.yaml\n    status: requested\n",
    )
    .unwrap();
    let design = directory.path().join("design-systems/edgar-v1");
    fs::create_dir_all(&design).unwrap();
    fs::write(
        design.join("design-system.yaml"),
        "schema: compositor.dev/design-system/v1\nid: edgar-v1\nname: Edgar\nversion: 1\n",
    )
    .unwrap();
    fs::write(
        design.join("spread-roles.yaml"),
        "roles:\n  opening:\n    energy: { min: 1, max: 1 }\n",
    )
    .unwrap();
    fs::write(
        design.join("validation-rules.yaml"),
        "page_turns: []\npacing: {}\n",
    )
    .unwrap();
    fs::write(
        design.join("layout-families.yaml"),
        "layout_families:\n  text:\n    variants:\n      standard: {}\n",
    )
    .unwrap();
    directory
}

#[test]
fn cli_reports_the_build_version_and_omits_legacy_commands() {
    let binary = env!("CARGO_BIN_EXE_compositor");
    let version = Command::new(binary).arg("--version").output().unwrap();
    assert!(version.status.success());
    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    for command in ["plan", "proof", "resolve", "reconcile", "diff"] {
        assert!(
            !help.contains(&format!("\n  {command} ")),
            "legacy command {command} remains visible"
        );
    }
}

#[test]
fn legacy_state_blocks_without_modifying_it() {
    let directory = package_project();
    let legacy = directory.path().join(".compositor/manifest.json");
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    fs::write(&legacy, "legacy state").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "build",
            "magic",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("legacy production state"));
    assert_eq!(fs::read_to_string(legacy).unwrap(), "legacy state");
}

#[test]
fn production_cli_rejects_removed_legacy_configuration() {
    let directory = package_project();
    let config = directory.path().join("compositor.toml");
    fs::write(
        &config,
        format!(
            "{}\n[pagination]\ntarget_words_per_text_page = 90\n",
            DEFAULT_CONFIG
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args(["--root", directory.path().to_str().unwrap(), "validate"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn package_build_emits_a_flow_composition_assembly_guide() {
    let directory = package_project();
    make_opener_art_ready(&directory);
    let output = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "build",
            "magic",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let guide = directory
        .path()
        .join("output/packages/magic/r01/01-story/assembly-guide.html");
    let guide = fs::read_to_string(guide).unwrap();
    assert!(guide.contains("opener-art"));
    assert!(guide.contains("spread-001"));
    assert!(guide.contains("Once upon a time."));
    assert!(!directory.path().join(".compositor").exists());
}

#[test]
fn explicit_output_requires_replace() {
    let directory = package_project();
    make_opener_art_ready(&directory);
    let output = directory.path().join("package");
    fs::create_dir_all(&output).unwrap();
    let failure = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "build",
            "magic",
            "story",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!failure.status.success());
    let success = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "build",
            "magic",
            "story",
            "--output",
            output.to_str().unwrap(),
            "--replace",
        ])
        .output()
        .unwrap();
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stderr)
    );
}

#[test]
fn validate_package_detects_tampered_art() {
    let directory = package_project();
    make_opener_art_ready(&directory);
    let output = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "build",
            "magic",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let package = directory.path().join("output/packages/magic/r01/01-story");
    let valid = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "validate-package",
            package.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    fs::write(package.join("opener/art/opener-art.png"), "tampered").unwrap();
    let invalid = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "validate-package",
            package.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
}

#[test]
fn migration_rolls_back_when_receipt_cannot_be_published() {
    let directory = package_project();
    let candidate = directory.path().join("assets/drafts/opener-art/a.png");
    fs::create_dir_all(candidate.parent().unwrap()).unwrap();
    image::RgbImage::new(1, 1).save(&candidate).unwrap();
    fs::write(
        directory.path().join("art/briefs/opener-art.yaml"),
        "schema_version: 2\nart_id: opener-art\nsource:\n  story_id: story\n  anchor_id: opening\nusage: opener\ngeneration:\n  page_treatment: floating\n  prompt: A test opener.\ncandidates:\n  - id: a\n    file: assets/drafts/opener-art/a.png\nselection:\n  candidate_id: a\n",
    )
    .unwrap();
    legacy_manifest(&directory, None);
    let brief_before =
        fs::read_to_string(directory.path().join("art/briefs/opener-art.yaml")).unwrap();
    let registry_before = fs::read_to_string(directory.path().join("art/assets.yaml")).unwrap();
    fs::write(directory.path().join("output"), "not a directory").unwrap();
    assert!(migration::run(directory.path(), MigrationOptions { apply: true }).is_err());
    assert_eq!(
        fs::read_to_string(directory.path().join("art/briefs/opener-art.yaml")).unwrap(),
        brief_before
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("art/assets.yaml")).unwrap(),
        registry_before
    );
}

fn make_opener_art_ready(directory: &tempfile::TempDir) {
    let candidate = directory.path().join("assets/drafts/opener-art/a.png");
    fs::create_dir_all(candidate.parent().unwrap()).unwrap();
    image::RgbImage::new(1, 1).save(&candidate).unwrap();
    fs::write(
        directory.path().join("art/briefs/opener-art.yaml"),
        "schema_version: 3\nart_id: opener-art\nsource:\n  story_id: story\n  anchor_id: opening\nusage: opener\ngeneration:\n  page_treatment: floating\n  prompt: A test opener.\ncandidates:\n  - id: a\n    file: assets/drafts/opener-art/a.png\n",
    )
    .unwrap();
    let hash =
        compositor::assets::sha256(directory.path(), "assets/drafts/opener-art/a.png").unwrap();
    fs::write(
        directory.path().join("art/assets.yaml"),
        format!(
            "schema: compositor.dev/art-assets/v2\nassets:\n  - id: opener-art\n    brief: art/briefs/opener-art.yaml\n    status: draft\n    selection:\n      candidate_id: a\n      file: assets/drafts/opener-art/a.png\n      sha256: {hash}\n"
        ),
    )
    .unwrap();
}

fn legacy_manifest(directory: &tempfile::TempDir, approved_art: Option<&str>) {
    let manifest = serde_json::json!({
        "schema_version": 2,
        "stories": { "story": { "units": [{
            "id": "opening", "anchor": "opening",
            "art_brief": "art/briefs/opener-art.yaml",
            "approved_art": approved_art
        }]}}
    });
    let path = directory.path().join(".compositor/manifest.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    compositor::storage::write_json_atomic(&path, &manifest).unwrap();
}

#[test]
fn legacy_migration_dry_run_is_non_mutating_and_apply_imports_review_selection() {
    let directory = package_project();
    let candidate = directory.path().join("assets/drafts/opener-art/a.png");
    fs::create_dir_all(candidate.parent().unwrap()).unwrap();
    image::RgbImage::new(1, 1).save(&candidate).unwrap();
    fs::write(
        directory.path().join("art/briefs/opener-art.yaml"),
        "schema_version: 2\nart_id: opener-art\nsource:\n  story_id: story\n  anchor_id: opening\nusage: opener\ngeneration:\n  page_treatment: floating\n  prompt: A test opener.\ncandidates:\n  - id: a\n    file: assets/drafts/opener-art/a.png\nselection:\n  candidate_id: a\n  feedback: Keep this direction.\n",
    )
    .unwrap();
    legacy_manifest(&directory, None);
    let config = directory.path().join("compositor.toml");
    fs::write(
        &config,
        format!("{}\n[state]\ndirectory = \".compositor\"\n", DEFAULT_CONFIG),
    )
    .unwrap();
    let brief_before =
        fs::read_to_string(directory.path().join("art/briefs/opener-art.yaml")).unwrap();
    let report = migration::run(directory.path(), MigrationOptions { apply: false }).unwrap();
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    assert_eq!(
        fs::read_to_string(directory.path().join("art/briefs/opener-art.yaml")).unwrap(),
        brief_before
    );
    assert!(!directory
        .path()
        .join("output/reports/legacy-production-migration.json")
        .exists());

    let report = migration::run(directory.path(), MigrationOptions { apply: true }).unwrap();
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    let brief = fs::read_to_string(directory.path().join("art/briefs/opener-art.yaml")).unwrap();
    assert!(brief.contains("schema_version: 3"));
    assert!(!brief.contains("selection:"));
    let registry = fs::read_to_string(directory.path().join("art/assets.yaml")).unwrap();
    assert!(registry.contains("status: review"));
    assert!(registry.contains("candidate_id: a"));
    assert!(directory.path().join(".compositor/manifest.json").is_file());
}

#[test]
fn legacy_migration_copies_a_verified_approved_asset() {
    let directory = package_project();
    let legacy_asset = directory.path().join("assets/legacy/opener.png");
    fs::create_dir_all(legacy_asset.parent().unwrap()).unwrap();
    image::RgbImage::new(1, 1).save(&legacy_asset).unwrap();
    fs::write(
        directory.path().join("art/briefs/opener-art.yaml"),
        "schema_version: 2\nart_id: opener-art\nsource:\n  story_id: story\n  anchor_id: opening\nusage: opener\ngeneration:\n  page_treatment: floating\n  prompt: A test opener.\n",
    )
    .unwrap();
    legacy_manifest(&directory, Some("assets/legacy/opener.png"));
    let report = migration::run(directory.path(), MigrationOptions { apply: true }).unwrap();
    assert!(report.blockers.is_empty(), "{:?}", report.blockers);
    let approved = directory.path().join("assets/approved/opener-art.png");
    assert!(approved.is_file());
    assert_eq!(
        compositor::assets::sha256(directory.path(), "assets/legacy/opener.png").unwrap(),
        compositor::assets::sha256(directory.path(), "assets/approved/opener-art.png").unwrap()
    );
    assert!(fs::read_to_string(directory.path().join("art/assets.yaml"))
        .unwrap()
        .contains("status: approved"));
    assert!(directory.path().join(".compositor/manifest.json").is_file());
}

#[test]
fn legacy_migration_rejects_unsupported_state_without_writing() {
    let directory = package_project();
    let manifest = directory.path().join(".compositor/manifest.json");
    fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    fs::write(&manifest, "{\"schema_version\": 99, \"stories\": {}}").unwrap();
    let registry_before = fs::read_to_string(directory.path().join("art/assets.yaml")).unwrap();
    let report = migration::run(directory.path(), MigrationOptions { apply: true }).unwrap();
    assert!(!report.blockers.is_empty());
    assert_eq!(
        fs::read_to_string(directory.path().join("art/assets.yaml")).unwrap(),
        registry_before
    );
    assert!(!directory
        .path()
        .join("output/reports/legacy-production-migration.json")
        .exists());
}

#[test]
fn legacy_migration_rejects_an_approved_target_hash_collision() {
    let directory = package_project();
    let legacy_asset = directory.path().join("assets/legacy/opener.png");
    fs::create_dir_all(legacy_asset.parent().unwrap()).unwrap();
    image::RgbImage::new(1, 1).save(&legacy_asset).unwrap();
    fs::write(
        directory.path().join("art/briefs/opener-art.yaml"),
        "schema_version: 2\nart_id: opener-art\nsource:\n  story_id: story\n  anchor_id: opening\nusage: opener\ngeneration:\n  page_treatment: floating\n  prompt: A test opener.\n",
    )
    .unwrap();
    let approved = directory.path().join("assets/approved/opener-art.png");
    fs::create_dir_all(approved.parent().unwrap()).unwrap();
    image::RgbImage::new(2, 2).save(&approved).unwrap();
    legacy_manifest(&directory, Some("assets/legacy/opener.png"));
    let report = migration::run(directory.path(), MigrationOptions { apply: true }).unwrap();
    assert!(report
        .blockers
        .iter()
        .any(|blocker| blocker.contains("overwrite")));
    assert!(
        fs::read_to_string(directory.path().join("art/briefs/opener-art.yaml"))
            .unwrap()
            .contains("schema_version: 2")
    );
    assert!(!directory
        .path()
        .join("output/reports/legacy-production-migration.json")
        .exists());
}
