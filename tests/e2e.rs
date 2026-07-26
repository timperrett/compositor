use compositor::config::DEFAULT_CONFIG;
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
    assert!(String::from_utf8_lossy(&version.stdout).contains("0.2.0"));
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
fn init_creates_reports_but_not_removed_proof_output() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args(["--root", directory.path().to_str().unwrap(), "init"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(directory.path().join("output/reports").is_dir());
    assert!(!directory.path().join("output/proofs").exists());
}

#[test]
fn art_dashboard_reports_readiness_without_changing_art_state() {
    let directory = package_project();
    make_opener_art_ready(&directory);
    fs::write(
        directory.path().join("art/assets.yaml"),
        "schema: compositor.dev/art-assets/v2\nassets:\n  - id: opener-art\n    brief: art/briefs/opener-art.yaml\n    status: requested\n",
    )
    .unwrap();
    let before = fs::read_to_string(directory.path().join("art/assets.yaml")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "--format",
            "json",
            "art",
            "dashboard",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["data"]["blocker_count"], 1);
    assert_eq!(
        report["data"]["output"],
        "output/reports/art-dashboard.html"
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("art/assets.yaml")).unwrap(),
        before
    );
    let html =
        fs::read_to_string(directory.path().join("output/reports/art-dashboard.html")).unwrap();
    assert!(html.contains("needs selection"));
    assert!(html.contains("compositor art select opener-art a"));
    assert!(html.contains("../../assets/drafts/opener-art/a.png"));
    assert!(html.contains("id=\"compendium\""));
    assert!(html.contains("<option value=\"magic\">magic</option>"));
    assert!(html.contains("data-compendium=\"magic\""));
    assert!(html.contains("Iowan Old Style"));

    let list = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "--format",
            "json",
            "art",
            "list",
        ])
        .output()
        .unwrap();
    assert!(list.status.success());
    let list: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(list["data"][0]["registry_status"], "requested");
    assert_eq!(list["data"][0]["readiness"], "needs selection");
    assert_eq!(list["data"][0]["default_policy_ready"], false);
}

#[test]
fn art_dashboard_supports_explicit_output_and_lifecycle_diagnostics() {
    let directory = package_project();
    make_opener_art_ready(&directory);
    let candidate = directory.path().join("assets/drafts/opener-art/a.png");
    image::RgbImage::new(2, 1).save(&candidate).unwrap();
    let registry_path = directory.path().join("art/assets.yaml");
    let mut registry = fs::read_to_string(&registry_path).unwrap();
    registry.push_str(
        "  - id: missing-brief\n    brief: art/briefs/missing-brief.yaml\n    status: requested\n",
    );
    fs::write(&registry_path, registry).unwrap();
    let invalid = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "art",
            "dashboard",
            "--output",
            "reports/nested/dashboard.html",
        ])
        .output()
        .unwrap();
    assert!(invalid.status.success());
    let html = fs::read_to_string(directory.path().join("reports/nested/dashboard.html")).unwrap();
    assert!(html.contains("invalid record"));
    assert!(html.contains("missing brief"));
    assert!(html.contains("unplaced/orphan"));
    assert!(html.contains("../../assets/drafts/opener-art/a.png"));

    let escape = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "art",
            "dashboard",
            "--output",
            "../dashboard.html",
        ])
        .output()
        .unwrap();
    assert!(!escape.status.success());
    assert!(String::from_utf8_lossy(&escape.stderr).contains("project-relative"));
}

#[test]
fn art_dashboard_marks_review_and_approved_art_ready() {
    let directory = package_project();
    make_opener_art_ready(&directory);
    let binary = env!("CARGO_BIN_EXE_compositor");
    for command in [
        ["art", "review", "opener-art"],
        ["art", "approve", "opener-art"],
    ] {
        let output = Command::new(binary)
            .args(["--root", directory.path().to_str().unwrap()])
            .args(command)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "art",
            "dashboard",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let html =
        fs::read_to_string(directory.path().join("output/reports/art-dashboard.html")).unwrap();
    assert!(html.contains(">approved<"));
    assert!(html.contains("<strong>0</strong><span>required blockers</span>"));
}

#[test]
fn website_uses_current_review_surface_terms() {
    let website = fs::read_to_string("website/index.html").unwrap();
    assert!(website.contains("assembly-guide.html"));
    assert!(!website.to_ascii_lowercase().contains("proof.html"));
    assert!(!website.to_ascii_lowercase().contains("html proof"));
}

#[test]
fn documentation_does_not_reintroduce_removed_production_surfaces() {
    for path in ["README.md", "website/index.html", "docs/art-protocol.md"] {
        let text = fs::read_to_string(path).unwrap().to_ascii_lowercase();
        for phrase in [
            "proof.html",
            "html proof",
            "compositor plan",
            "compositor approve",
        ] {
            assert!(
                !text.contains(phrase),
                "{path} contains removed term {phrase}"
            );
        }
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
fn package_build_reports_requested_art_policy_failures_before_exiting() {
    let directory = package_project();
    let output = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "build",
            "magic",
            "--asset-policy",
            "draft",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ART_STATUS_BELOW_POLICY"), "{stdout}");
    assert!(
        stdout.contains("asset `opener-art` is `requested`"),
        "{stdout}"
    );
    assert!(
        stdout.contains("compositor art select opener-art <candidate-id>"),
        "{stdout}"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("validation failed"));
    assert!(!directory
        .path()
        .join("output/packages/magic/r01/01-story")
        .exists());

    let json_directory = package_project();
    let output = Command::new(env!("CARGO_BIN_EXE_compositor"))
        .args([
            "--root",
            json_directory.path().to_str().unwrap(),
            "--format",
            "json",
            "build",
            "magic",
            "--asset-policy",
            "draft",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["data"]["revision"], serde_json::Value::Null);
    assert_eq!(report["data"]["outputs"], serde_json::json!([]));
    assert!(report["validation"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == "ART_STATUS_BELOW_POLICY"));
    assert!(!json_directory
        .path()
        .join("output/packages/magic/r01/01-story")
        .exists());
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
