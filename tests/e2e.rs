use compositor::build;
use compositor::config::{Config, DEFAULT_CONFIG};
use compositor::model::{ChangeKind, IllustrationRequirement};
use compositor::storage;
use std::fs;
use std::process::Command;

#[test]
fn cli_reports_the_build_version() {
    let binary = env!("CARGO_BIN_EXE_compositor");
    let output = Command::new(binary).arg("--version").output().unwrap();

    assert!(output.status.success());
    let version = String::from_utf8(output.stdout).unwrap();
    assert!(
        version.trim().starts_with("compositor "),
        "unexpected version output: {version}"
    );
    let value = version.trim().strip_prefix("compositor ").unwrap();
    let (date, revision) = value.rsplit_once('-').unwrap();
    assert!(
        date.len() == 6
            && date.as_bytes()[2] == b'.'
            && date
                .bytes()
                .enumerate()
                .all(|(index, byte)| index == 2 || byte.is_ascii_digit()),
        "unexpected build date: {date}"
    );
    assert!(
        !revision.is_empty() && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "unexpected git revision: {revision}"
    );
}

#[test]
fn cli_tree_lists_ordered_compendiums_stories_and_optional_art_ids() {
    let directory = package_project();
    let binary = env!("CARGO_BIN_EXE_compositor");

    let tree = Command::new(binary)
        .args(["--root", directory.path().to_str().unwrap(), "tree"])
        .output()
        .unwrap();
    assert!(tree.status.success());
    assert_eq!(
        String::from_utf8(tree.stdout).unwrap(),
        concat!(
            "compendiums\n",
            "└── Magic [magic]\n",
            "    ├── First [first]\n",
            "    └── Second [second]\n",
        )
    );

    let tree_with_art = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "tree",
            "--art",
        ])
        .output()
        .unwrap();
    assert!(tree_with_art.status.success());
    assert_eq!(
        String::from_utf8(tree_with_art.stdout).unwrap(),
        concat!(
            "compendiums\n",
            "└── Magic [magic]\n",
            "    ├── First [first]\n",
            "    │   └── art: first-opener\n",
            "    └── Second [second]\n",
            "        └── art: second-opener\n",
        )
    );

    let json = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "--format",
            "json",
            "tree",
            "--art",
        ])
        .output()
        .unwrap();
    assert!(json.status.success());
    let output: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(output["command"], "tree");
    assert_eq!(
        output["data"]["compendiums"][0]["stories"][0]["art_ids"],
        serde_json::json!(["first-opener"])
    );
}

fn project() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("compositor.toml"), DEFAULT_CONFIG).unwrap();
    let compendium = directory.path().join("compendiums/01-magic");
    fs::create_dir_all(&compendium).unwrap();
    fs::write(
        compendium.join("index.md"),
        "---\nid: magic\ntitle: Magic\n---\nA collection.\n",
    )
    .unwrap();
    let story = compendium.join("01-story");
    fs::create_dir_all(&story).unwrap();
    fs::write(story.join("story.md"), "---\nid: story\ntitle: Story\n---\n<!-- anchor: opening -->\nOnce upon a time.\n\n---\nA second unit stays the same.\n").unwrap();
    directory
}

fn package_project() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("compositor.toml"), DEFAULT_CONFIG).unwrap();
    let compendium = directory.path().join("compendiums/01-magic");
    fs::create_dir_all(&compendium).unwrap();
    fs::write(
        compendium.join("index.md"),
        "---\nid: magic\ntitle: Magic\n---\nA collection.\n",
    )
    .unwrap();
    for (directory_name, id, title) in [
        ("01-first", "first", "First"),
        ("02-second", "second", "Second"),
    ] {
        let story_directory = compendium.join(directory_name);
        fs::create_dir_all(&story_directory).unwrap();
        let story_path = story_directory.join("story.md");
        fs::write(
            &story_path,
            format!(
                "---\nid: {id}\ntitle: {title}\n---\n<!-- anchor: opening -->\n<!-- paragraph: {id}-opening -->\n\nOnce upon a time.\n"
            ),
        )
        .unwrap();
        let story = compositor::flow::load_story(&story_path).unwrap();
        fs::write(
            story_directory.join("story.flow.yaml"),
            format!(
                "schema: compositor.dev/story-flow/v1\nstory:\n  id: {id}\n  source_revision: {}\nspreads:\n  - id: spread-001\n    source:\n      from: {{ type: paragraph, id: {id}-opening }}\n      through: {{ type: paragraph, id: {id}-opening }}\n    role: opening\n    energy: 1\n    narrative: {{ purpose: Open the story. }}\n",
                story.source_hash
            ),
        )
        .unwrap();
        fs::write(
            story_directory.join("hardcover.composition.yaml"),
            format!(
                "schema: compositor.dev/composition-plan/v2\nstory:\n  id: {id}\n  flow: story.flow.yaml\nedition:\n  id: hardcover\n  design_system: edgar-v1\nopener:\n  title: {title}\n  placement: center-page\n  art: {{ id: {id}-opener, role: primary-subject }}\nspreads:\n  - id: spread-001\n    layout: {{ family: text, variant: standard }}\n    text: {{ density: standard }}\n    illustration: {{ mode: none, focal_subject: none }}\n"
            ),
        )
        .unwrap();
        let brief_directory = directory.path().join("art/briefs");
        fs::create_dir_all(&brief_directory).unwrap();
        fs::write(
            brief_directory.join(format!("{id}-opener.yaml")),
            format!(
                "schema_version: 2\nart_id: {id}-opener\nsource:\n  story_id: {id}\n  anchor_id: opening\nusage: opener\ngeneration:\n  page_treatment: floating\n  prompt: A test opener.\n"
            ),
        )
        .unwrap();
    }
    fs::write(
        directory.path().join("art/assets.yaml"),
        "schema: compositor.dev/art-assets/v1\nassets: []\n",
    )
    .unwrap();
    let design_system = directory.path().join("design-systems/edgar-v1");
    fs::create_dir_all(&design_system).unwrap();
    fs::write(
        design_system.join("design-system.yaml"),
        "schema: compositor.dev/design-system/v1\nid: edgar-v1\nname: Edgar\nversion: 1\n",
    )
    .unwrap();
    fs::write(design_system.join("spread-roles.yaml"), "roles: {}\n").unwrap();
    fs::write(
        design_system.join("layout-families.yaml"),
        "layout_families:\n  text:\n    variants:\n      standard: {}\n",
    )
    .unwrap();
    directory
}

#[test]
fn art_coverage_reports_legacy_briefs_as_needing_mapping() {
    let directory = package_project();
    fs::write(
        directory.path().join("art/briefs/first-scene.yaml"),
        "schema_version: 2\nart_id: first-scene\nsource:\n  story_id: first\n  anchor_id: opening\ngeneration:\n  page_treatment: floating\n  prompt: A first-story scene.\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("art/assets.yaml"),
        "schema: compositor.dev/art-assets/v1\nassets:\n  - id: first-opener\n    brief: art/briefs/first-opener.yaml\n    status: requested\n  - id: first-scene\n    brief: art/briefs/first-scene.yaml\n    status: requested\n",
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_compositor");
    let output = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "--format",
            "json",
            "art",
            "coverage",
            "--story",
            "first",
            "--edition",
            "hardcover",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "{:?}", output);
    let body = String::from_utf8(output.stdout).unwrap();
    assert!(body.contains("needs-mapping"), "{body}");
    assert!(body.contains("first-scene"), "{body}");
}

fn words(count: usize) -> String {
    (0..count).map(|_| "word").collect::<Vec<_>>().join(" ")
}

fn replace_story_with_units(directory: &tempfile::TempDir, unit_words: &[usize]) {
    let units = unit_words
        .iter()
        .enumerate()
        .map(|(index, count)| format!("<!-- anchor: unit-{index} -->\n{}", words(*count)))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    fs::write(
        directory
            .path()
            .join("compendiums/01-magic/01-story/story.md"),
        format!("---\nid: story\ntitle: Story\n---\n{units}\n"),
    )
    .unwrap();
}

fn replace_story_with_text(directory: &tempfile::TempDir, text: &str) {
    fs::write(
        directory
            .path()
            .join("compendiums/01-magic/01-story/story.md"),
        format!("---\nid: story\ntitle: Story\n---\n<!-- anchor: unit-0 -->\n{text}\n"),
    )
    .unwrap();
}

fn sentence(count: usize) -> String {
    let mut values = (0..count.saturating_sub(1))
        .map(|_| "word")
        .collect::<Vec<_>>();
    values.push("sentence.");
    values.join(" ")
}

fn pagination_config(directory: &tempfile::TempDir, target: usize, maximum: usize) -> Config {
    let path = directory.path().join("compositor.toml");
    let updated = fs::read_to_string(&path)
        .unwrap()
        .replace(
            "target_words_per_text_page = 90",
            &format!("target_words_per_text_page = {target}"),
        )
        .replace(
            "maximum_words_per_text_page = 130",
            &format!("maximum_words_per_text_page = {maximum}"),
        );
    fs::write(path, updated).unwrap();
    Config::load(directory.path()).unwrap()
}

#[test]
fn unchanged_build_is_a_no_op() {
    let directory = project();
    let config = Config::load(directory.path()).unwrap();
    let (_, first, first_plans) = build::build(directory.path(), &config, None).unwrap();
    assert_eq!(first.unwrap().revision, 1);
    assert_eq!(first_plans.len(), 1);
    let (_, second, plans) = build::build(directory.path(), &config, None).unwrap();
    assert!(second.is_none());
    assert!(plans.is_empty());
    let manifest = storage::load_manifest(directory.path(), &config)
        .unwrap()
        .unwrap();
    assert_eq!(manifest.revision, 1);
}

#[test]
fn build_generates_plain_text_layout_exports_without_markdown() {
    let directory = project();
    fs::write(
        directory.path().join("compendiums/01-magic/01-story/story.md"),
        "---\nid: story\ntitle: Story\n---\n<!-- anchor: opening -->\n# A Story Display Title\n\n# A **bold** beginning\n\nA [linked](https://example.com) paragraph.\n\n- First item\n- Second item\n\n---\n\n> A closing quotation.\n",
    )
    .unwrap();
    let second = directory.path().join("compendiums/01-magic/02-second");
    fs::create_dir_all(&second).unwrap();
    fs::write(
        second.join("story.md"),
        "---\nid: second\ntitle: Second Story\n---\n# Magic\n\nSecond body.\n",
    )
    .unwrap();
    let config = Config::load(directory.path()).unwrap();

    build::build(directory.path(), &config, None).unwrap();

    let story = fs::read_to_string(directory.path().join("output/text/story.txt")).unwrap();
    assert_eq!(
        story,
        "A bold beginning\n\nA linked paragraph.\n\n• First item\n• Second item\n\nA closing quotation.\n"
    );
    assert!(!story.contains("A Story Display Title"));
    assert!(!story.contains("<!--"));
    assert!(!story.contains("**"));
    assert!(!story.contains("[linked]("));
    let compendium = fs::read_to_string(directory.path().join("output/text/magic.txt")).unwrap();
    assert_eq!(compendium, format!("{story}\n\nSecond body.\n"));
    assert!(!compendium.contains("Magic"));
    assert!(!compendium.contains("Second Story"));
    assert_eq!(
        fs::read_to_string(directory.path().join("output/text/second.txt")).unwrap(),
        "Second body.\n"
    );
}

#[test]
fn unchanged_build_restores_missing_plain_text_export_and_source_edits_refresh_it() {
    let directory = project();
    let config = Config::load(directory.path()).unwrap();
    build::build(directory.path(), &config, None).unwrap();
    let source = directory
        .path()
        .join("compendiums/01-magic/01-story/story.md");
    let original_source = fs::read_to_string(&source).unwrap();
    let export = directory.path().join("output/text/story.txt");
    fs::remove_file(&export).unwrap();

    build::build(directory.path(), &config, None).unwrap();

    assert_eq!(fs::read_to_string(&source).unwrap(), original_source);
    assert!(export.is_file());
    fs::write(
        &source,
        original_source.replace("Once upon a time.", "Once upon a sunny time."),
    )
    .unwrap();
    build::build(directory.path(), &config, None).unwrap();
    assert!(fs::read_to_string(export)
        .unwrap()
        .contains("Once upon a sunny time."));
}

#[test]
fn local_edit_keeps_anchored_unit_identity() {
    let directory = project();
    let config = Config::load(directory.path()).unwrap();
    build::build(directory.path(), &config, None).unwrap();
    let story = directory
        .path()
        .join("compendiums/01-magic/01-story/story.md");
    let source = fs::read_to_string(&story)
        .unwrap()
        .replace("Once upon a time.", "Once upon a sunny time.");
    fs::write(story, source).unwrap();
    let (prepared, manifest, _) = build::build(directory.path(), &config, None).unwrap();
    assert_eq!(manifest.unwrap().revision, 2);
    assert!(prepared
        .changes
        .changes
        .iter()
        .any(|change| change.unit_id.as_deref() == Some("opening")
            && change.kind == ChangeKind::Edited));
    assert!(prepared
        .changes
        .changes
        .iter()
        .any(|change| change.kind == ChangeKind::Unchanged));
}

#[test]
fn conservative_plan_keeps_unaffected_assignments_on_their_existing_pages() {
    let directory = project();
    replace_story_with_units(&directory, &[100, 100]);
    let config = Config::load(directory.path()).unwrap();
    build::build(directory.path(), &config, None).unwrap();
    let story = directory
        .path()
        .join("compendiums/01-magic/01-story/story.md");
    fs::write(
        &story,
        fs::read_to_string(&story).unwrap().replacen(
            "<!-- anchor: unit-0 -->\nword",
            "<!-- anchor: unit-0 -->\nchanged",
            1,
        ),
    )
    .unwrap();
    let (_, _, plans) = build::build(directory.path(), &config, None).unwrap();
    let plan = &plans[0];
    assert!(plan
        .assignments
        .iter()
        .any(|assignment| { assignment.pages == [2] && assignment.units == ["unit-1"] }));
    assert!(plan
        .assignments
        .iter()
        .any(|assignment| { assignment.pages == [3] && assignment.units == ["unit-0"] }));
}

#[test]
fn artwork_requirements_are_generated_without_legacy_briefs() {
    let directory = project();
    fs::write(
        directory.path().join("compendiums/01-magic/01-story/story.md"),
        "---\nid: story\ntitle: Story\n---\n<!-- anchor: reveal -->\n<!-- art: A moonlit library. -->\n<!-- layout: full-spread -->\nEdgar opens the book.\n",
    )
    .unwrap();
    let config = Config::load(directory.path()).unwrap();
    let (_, _, plans) = build::build(directory.path(), &config, None).unwrap();
    assert_eq!(plans[0].assignments[0].art_id.as_deref(), Some("reveal"));
    let requirement: IllustrationRequirement = storage::read_json(
        &directory
            .path()
            .join(".compositor/requirements/reveal/v001-candidate.json"),
    )
    .unwrap();
    assert_eq!(requirement.pages, vec![1, 2]);
    assert_eq!(requirement.art_note.as_deref(), Some("A moonlit library."));
    assert!(!directory.path().join(".compositor/briefs").exists());
}

#[test]
fn art_layout_controls_surface_and_requirement_geometry() {
    let directory = project();
    fs::write(
        directory.path().join("compendiums/01-magic/01-story/story.md"),
        "---\nid: story\ntitle: Story\n---\n<!-- anchor: page-art -->\n<!-- art: A page illustration. -->\n<!-- art-layout: surface=single-page orientation=portrait height=50% -->\nPage art.\n\n---\n\n<!-- anchor: spread-art -->\n<!-- art: A spread illustration. -->\n<!-- art-layout: surface=double-page-spread orientation=landscape height=50% -->\nSpread art.\n",
    )
    .unwrap();
    let config = Config::load(directory.path()).unwrap();
    let (_, _, plans) = build::build(directory.path(), &config, None).unwrap();
    assert_eq!(plans[0].assignments[0].pages, vec![1]);
    assert_eq!(plans[0].assignments[1].pages, vec![2, 3]);

    let page: IllustrationRequirement = storage::read_json(
        &directory
            .path()
            .join(".compositor/requirements/page-art/v001-candidate.json"),
    )
    .unwrap();
    let page_geometry = page.geometry.unwrap();
    assert_eq!(page_geometry.width_px, 1200);
    assert_eq!(page_geometry.height_px, 1500);
    assert!((page_geometry.aspect_ratio - 0.8).abs() < 0.0001);

    let spread: IllustrationRequirement = storage::read_json(
        &directory
            .path()
            .join(".compositor/requirements/spread-art/v001-candidate.json"),
    )
    .unwrap();
    let spread_geometry = spread.geometry.unwrap();
    assert_eq!(spread_geometry.width_px, 4800);
    assert_eq!(spread_geometry.height_px, 1500);

    let mut changed_config = config.clone();
    changed_config.book.trim_width_in = 7.0;
    build::build(directory.path(), &changed_config, None).unwrap();
    let revised_page =
        storage::load_latest_requirement(directory.path(), &changed_config, "page-art")
            .unwrap()
            .unwrap();
    assert_eq!(revised_page.revision, 2);
    assert_eq!(revised_page.geometry.unwrap().width_px, 1050);
}

#[test]
fn selected_art_brief_candidate_is_promoted_and_used_in_proof() {
    let directory = project();
    fs::write(
        directory.path().join("compendiums/01-magic/01-story/story.md"),
        "---\nid: story\ntitle: Story\n---\n<!-- anchor: reveal -->\n<!-- layout: full-page -->\nEdgar opens the book.\n",
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_compositor");
    let build = Command::new(binary)
        .args(["--root", directory.path().to_str().unwrap(), "build"])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    fs::create_dir_all(directory.path().join("art/briefs")).unwrap();
    fs::create_dir_all(directory.path().join("assets/drafts/reveal/r01")).unwrap();
    fs::write(
        directory
            .path()
            .join("assets/drafts/reveal/r01/candidate-a.png"),
        b"not-an-image",
    )
    .unwrap();
    fs::write(
        directory.path().join("art/briefs/reveal.yaml"),
        "schema_version: 2\nart_id: reveal\nsource:\n  story_id: story\n  anchor_id: reveal\ngeneration:\n  page_treatment: spot\n  prompt: A moonlit library.\ncandidates:\n  - id: a\n    file: assets/drafts/reveal/r01/candidate-a.png\nselection:\n  candidate_id: a\n",
    )
    .unwrap();
    let validate = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "art",
            "validate",
            "--strict",
        ])
        .output()
        .unwrap();
    assert!(
        validate.status.success(),
        "{}",
        String::from_utf8_lossy(&validate.stderr)
    );
    let list = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "--format",
            "json",
            "art",
            "list",
            "--story",
            "story",
        ])
        .output()
        .unwrap();
    assert!(
        list.status.success(),
        "{}",
        String::from_utf8_lossy(&list.stderr)
    );
    let list = String::from_utf8(list.stdout).unwrap();
    assert!(list.contains("\"command\": \"art list\""));
    assert!(list.contains("\"art_id\": \"reveal\""));
    assert!(list.contains("\"art_brief\""));
    assert!(list.contains("\"selected_candidate\": \"a\""));

    let inspect = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "--format",
            "json",
            "art",
            "inspect",
            "reveal",
        ])
        .output()
        .unwrap();
    assert!(
        inspect.status.success(),
        "{}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    assert!(String::from_utf8(inspect.stdout)
        .unwrap()
        .contains("\"page_treatment\": \"spot\""));
    let brief = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "--format",
            "json",
            "art",
            "brief",
            "reveal",
        ])
        .output()
        .unwrap();
    assert!(
        brief.status.success(),
        "{}",
        String::from_utf8_lossy(&brief.stderr)
    );
    let brief = String::from_utf8(brief.stdout).unwrap();
    assert!(brief.contains("art/briefs/reveal.yaml"));
    assert!(brief.contains("\"page_treatment\": \"spot\""));

    let attach = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "art",
            "attach",
            "reveal",
            "--selected",
        ])
        .output()
        .unwrap();
    assert!(
        attach.status.success(),
        "{}",
        String::from_utf8_lossy(&attach.stderr)
    );
    let proof = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "proof",
            "--story",
            "story",
        ])
        .output()
        .unwrap();
    assert!(
        proof.status.success(),
        "{}",
        String::from_utf8_lossy(&proof.stderr)
    );
    assert!(
        fs::read_to_string(directory.path().join("output/proofs/story.html"))
            .unwrap()
            .contains("../../assets/approved/reveal-a.png")
    );

    let removed_attach = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "attach-art",
            "reveal",
            "assets/approved/reveal.png",
        ])
        .output()
        .unwrap();
    assert!(!removed_attach.status.success());
    let removed_inspect = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "inspect",
            "art",
            "reveal",
        ])
        .output()
        .unwrap();
    assert!(!removed_inspect.status.success());
}

#[test]
fn strict_art_validation_rejects_missing_legacy_or_unknown_page_treatments() {
    let directory = project();
    fs::write(
        directory.path().join("compendiums/01-magic/01-story/story.md"),
        "---\nid: story\ntitle: Story\n---\n<!-- anchor: reveal -->\n<!-- art: A moonlit library. -->\nEdgar opens the book.\n",
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_compositor");
    let build = Command::new(binary)
        .args(["--root", directory.path().to_str().unwrap(), "build"])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    fs::create_dir_all(directory.path().join("art/briefs")).unwrap();

    for treatment in [
        None,
        Some("  bleed_mode: contained\\n"),
        Some("  page_treatment: bordered\\n"),
    ] {
        let treatment = treatment.map(str::to_owned).unwrap_or_default();
        fs::write(
            directory.path().join("art/briefs/reveal.yaml"),
            format!(
                "schema_version: 2\nart_id: reveal\nsource:\n  story_id: story\n  anchor_id: reveal\ngeneration:\n{treatment}  prompt: A moonlit library.\n"
            ),
        )
        .unwrap();
        let validate = Command::new(binary)
            .args([
                "--root",
                directory.path().to_str().unwrap(),
                "art",
                "validate",
                "--strict",
            ])
            .output()
            .unwrap();
        assert!(!validate.status.success());
    }
}

#[test]
fn asset_registry_selection_review_and_approval_are_explicit() {
    let directory = project();
    fs::write(
        directory.path().join("compendiums/01-magic/01-story/story.md"),
        "---\nid: story\ntitle: Story\n---\n<!-- anchor: reveal -->\n<!-- art: A moonlit library. -->\nEdgar opens the book.\n",
    )
    .unwrap();
    fs::create_dir_all(directory.path().join("art/briefs")).unwrap();
    fs::create_dir_all(directory.path().join("assets/drafts/reveal/r01")).unwrap();
    fs::write(
        directory
            .path()
            .join("assets/drafts/reveal/r01/candidate-a.png"),
        b"candidate",
    )
    .unwrap();
    fs::write(
        directory.path().join("art/briefs/reveal.yaml"),
        "schema_version: 2\nart_id: reveal\nsource:\n  story_id: story\n  anchor_id: reveal\ngeneration:\n  page_treatment: spot\n  prompt: A moonlit library.\ncandidates:\n  - id: a\n    file: assets/drafts/reveal/r01/candidate-a.png\n",
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_compositor");
    for arguments in [
        vec!["art", "registry", "--write"],
        vec!["art", "select", "reveal", "a"],
        vec!["art", "review", "reveal"],
        vec!["art", "approve-asset", "reveal"],
    ] {
        let result = Command::new(binary)
            .args(["--root", directory.path().to_str().unwrap()])
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    assert_eq!(
        fs::read(directory.path().join("assets/approved/reveal.png")).unwrap(),
        b"candidate"
    );
    assert!(fs::read_to_string(directory.path().join("art/assets.yaml"))
        .unwrap()
        .contains("status: approved"));
}

#[test]
fn plan_diff_writes_a_visual_review_artifact() {
    let directory = project();
    replace_story_with_units(&directory, &[60, 60, 60]);
    let initial = Config::load(directory.path()).unwrap();
    build::build(directory.path(), &initial, None).unwrap();
    let tighter = pagination_config(&directory, 90, 100);
    build::build(directory.path(), &tighter, None).unwrap();
    let binary = env!("CARGO_BIN_EXE_compositor");
    let result = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "diff",
            "plan",
            "story",
            "v001",
            "v002",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(directory
        .path()
        .join("output/reports/story-v001-v002-plan-diff.html")
        .is_file());
}

#[test]
fn pagination_config_change_rebuilds_the_plan_without_rewriting_the_manifest() {
    let directory = project();
    replace_story_with_units(&directory, &[60, 60, 60]);
    let initial_config = Config::load(directory.path()).unwrap();
    let (_, manifest, plans) = build::build(directory.path(), &initial_config, None).unwrap();
    assert_eq!(manifest.unwrap().revision, 1);
    assert_eq!(plans[0].assignments.len(), 2);
    assert_eq!(plans[0].assignments[0].word_count, 120);

    let tighter_config = pagination_config(&directory, 90, 100);
    let (_, manifest, plans) = build::build(directory.path(), &tighter_config, None).unwrap();
    assert!(manifest.is_none());
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].revision, 2);
    assert_eq!(
        plans[0]
            .assignments
            .iter()
            .map(|assignment| assignment.word_count)
            .collect::<Vec<_>>(),
        vec![60, 60, 60]
    );

    let (_, manifest, plans) = build::build(directory.path(), &tighter_config, None).unwrap();
    assert!(manifest.is_none());
    assert!(plans.is_empty());
    assert_eq!(
        storage::load_manifest(directory.path(), &tighter_config)
            .unwrap()
            .unwrap()
            .revision,
        1
    );
}

#[test]
fn target_words_per_page_changes_text_page_packing() {
    let directory = project();
    replace_story_with_units(&directory, &[40, 40, 40]);
    let initial_config = Config::load(directory.path()).unwrap();
    let (_, _, plans) = build::build(directory.path(), &initial_config, None).unwrap();
    assert_eq!(
        plans[0]
            .assignments
            .iter()
            .map(|assignment| assignment.word_count)
            .collect::<Vec<_>>(),
        vec![80, 40]
    );

    let roomier_config = pagination_config(&directory, 120, 130);
    let (_, manifest, plans) = build::build(directory.path(), &roomier_config, None).unwrap();
    assert!(manifest.is_none());
    assert_eq!(plans[0].revision, 2);
    assert_eq!(plans[0].assignments.len(), 1);
    assert_eq!(plans[0].assignments[0].word_count, 120);
}

#[test]
fn oversized_unit_is_split_into_target_sized_page_fragments() {
    let directory = project();
    replace_story_with_text(
        &directory,
        &[sentence(38), sentence(40), sentence(40), sentence(17)].join("\n\n"),
    );
    let config = pagination_config(&directory, 40, 90);
    let (_, manifest, plans) = build::build(directory.path(), &config, None).unwrap();
    let plan = &plans[0];
    assert_eq!(manifest.unwrap().revision, 1);
    assert_eq!(
        plan.assignments
            .iter()
            .map(|assignment| assignment.word_count)
            .collect::<Vec<_>>(),
        vec![38, 40, 40, 17]
    );
    assert!(plan
        .assignments
        .iter()
        .all(|assignment| assignment.word_count <= 40));
    assert_eq!(plan.assignments[0].fragments[0].start_word, 0);
    assert_eq!(plan.assignments[0].fragments[0].end_word, 38);
    assert_eq!(plan.assignments[3].fragments[0].start_word, 118);
    assert_eq!(plan.assignments[3].fragments[0].end_word, 135);
    assert!(plan.warnings.is_empty());
}

#[test]
fn sentence_boundary_prevents_a_dangling_word_page() {
    let directory = project();
    replace_story_with_text(&directory, &sentence(41));
    let config = pagination_config(&directory, 40, 90);
    let (_, _, plans) = build::build(directory.path(), &config, None).unwrap();
    assert_eq!(plans[0].assignments.len(), 1);
    assert_eq!(plans[0].assignments[0].word_count, 41);
    assert_eq!(plans[0].assignments[0].fragments[0].end_word, 41);
}

#[test]
fn overlong_sentence_uses_the_maximum_as_a_last_resort() {
    let directory = project();
    replace_story_with_text(&directory, &words(95));
    let config = pagination_config(&directory, 40, 90);
    let (_, _, plans) = build::build(directory.path(), &config, None).unwrap();
    assert_eq!(
        plans[0]
            .assignments
            .iter()
            .map(|assignment| assignment.word_count)
            .collect::<Vec<_>>(),
        vec![90, 5]
    );
    assert!(plans[0].warnings[0].contains("no sentence or paragraph boundary"));
}

#[test]
fn invalid_pagination_capacity_is_rejected() {
    let directory = project();
    let path = directory.path().join("compositor.toml");
    let updated = fs::read_to_string(&path).unwrap().replace(
        "target_words_per_text_page = 90",
        "target_words_per_text_page = 131",
    );
    fs::write(path, updated).unwrap();
    let error = Config::load(directory.path()).unwrap_err();
    assert!(error
        .to_string()
        .contains("target_words_per_text_page must not exceed"));
}

#[test]
fn cli_initializes_and_builds_json_output() {
    let directory = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_compositor");
    let init = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "--format",
            "json",
            "init",
        ])
        .output()
        .unwrap();
    assert!(init.status.success());
    assert!(String::from_utf8(init.stdout)
        .unwrap()
        .contains("\"command\": \"init\""));
    let readme = fs::read_to_string(directory.path().join("README.md")).unwrap();
    assert!(readme.contains("## Directory guide"));
    assert!(readme.contains("`.compositor/`"));
    let compendium = directory.path().join("compendiums/01-magic");
    fs::create_dir_all(&compendium).unwrap();
    fs::write(
        compendium.join("index.md"),
        "---\nid: magic\ntitle: Magic\n---\nNotes.\n",
    )
    .unwrap();
    fs::create_dir_all(compendium.join("01-story")).unwrap();
    fs::write(
        compendium.join("01-story/story.md"),
        "---\nid: story\ntitle: Story\n---\nText.\n",
    )
    .unwrap();
    let build = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "--format",
            "json",
            "build",
        ])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let build_output = String::from_utf8(build.stdout).unwrap();
    assert!(build_output.contains("\"command\": \"build\""));
    assert!(build_output.contains("\"text_exports\""));
    assert!(build_output.contains("output/text/story.txt"));
}

#[test]
fn cli_inspects_and_validates_a_story_flow_plan() {
    let directory = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_compositor");
    let story = directory.path().join("story.md");
    fs::write(
        &story,
        "---\nid: map\ntitle: The Map\n---\n<!-- paragraph: opening-rain -->\n\nRain whispered.\n\n<!-- paragraph: map-revealed -->\n\nThe map shone.\n",
    )
    .unwrap();
    let inspect = Command::new(binary)
        .args(["--format", "json", "inspect", story.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(inspect.status.success());
    let inspection: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(inspection["command"], "inspect");
    assert_eq!(
        inspection["data"]["paragraphs"].as_array().unwrap().len(),
        2
    );
    let revision = inspection["data"]["source_revision"].as_str().unwrap();

    let design = directory.path().join("design-system");
    fs::create_dir_all(&design).unwrap();
    fs::write(
        design.join("design-system.yaml"),
        "schema: compositor.dev/design-system/v1\nid: edgar-v1\nname: Edgar\nversion: 1\n",
    )
    .unwrap();
    fs::write(
        design.join("spread-roles.yaml"),
        "roles:\n  opening-wonder:\n    energy: { min: 1, max: 3 }\n  reveal:\n    energy: { min: 4, max: 5 }\n",
    )
    .unwrap();
    fs::write(
        design.join("validation-rules.yaml"),
        "page_turns: [reveal]\npacing: { high_energy_threshold: 4, max_consecutive_high_energy: 2 }\n",
    )
    .unwrap();
    let flow = directory.path().join("story.flow.yaml");
    fs::write(
        &flow,
        format!(
            "schema: compositor.dev/story-flow/v1\nstory:\n  id: map\n  source_revision: {revision}\nspreads:\n  - id: spread-001\n    source:\n      from: {{ type: paragraph, id: opening-rain }}\n      through: {{ type: paragraph, id: opening-rain }}\n    role: opening-wonder\n    energy: 2\n    narrative: {{ purpose: Open the library. }}\n  - id: spread-002\n    source:\n      from: {{ type: paragraph, id: map-revealed }}\n      through: {{ type: paragraph, id: map-revealed }}\n    role: reveal\n    energy: 4\n    narrative: {{ purpose: Reveal the map. }}\n"
        ),
    )
    .unwrap();
    let validation = Command::new(binary)
        .args([
            "--format",
            "json",
            "validate-flow",
            story.to_str().unwrap(),
            flow.to_str().unwrap(),
            "--design-system",
            design.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        validation.status.success(),
        "{}",
        String::from_utf8_lossy(&validation.stderr)
    );
    assert!(String::from_utf8(validation.stdout)
        .unwrap()
        .contains("validate-flow"));
}

#[test]
fn package_build_uses_conventional_story_inputs_and_auto_revisions() {
    let directory = package_project();
    let binary = env!("CARGO_BIN_EXE_compositor");
    let first = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "--format",
            "json",
            "build",
            "magic",
        ])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_report: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first_report["data"]["revision"], "r01");
    assert_eq!(first_report["data"]["outputs"].as_array().unwrap().len(), 2);
    assert!(directory
        .path()
        .join("output/packages/magic/r01/01-first/manifest.yaml")
        .is_file());
    assert!(directory
        .path()
        .join("output/packages/magic/r01/02-second/manifest.yaml")
        .is_file());

    let second = Command::new(binary)
        .args([
            "--root",
            directory.path().to_str().unwrap(),
            "--format",
            "json",
            "build",
            "01-magic",
            "second",
        ])
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_report: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(second_report["data"]["revision"], "r02");
    assert_eq!(
        second_report["data"]["outputs"].as_array().unwrap().len(),
        1
    );
    assert!(directory
        .path()
        .join("output/packages/magic/r02/02-second/manifest.yaml")
        .is_file());
}

#[test]
fn package_validation_detects_stale_text_flow_and_composition() {
    let directory = package_project();
    let binary = env!("CARGO_BIN_EXE_compositor");
    let root = directory.path().to_str().unwrap();
    let build = Command::new(binary)
        .args(["--root", root, "build", "magic", "first"])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let package = directory.path().join("output/packages/magic/r01/01-first");

    let fresh = Command::new(binary)
        .args([
            "--root",
            root,
            "validate-package",
            package.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        fresh.status.success(),
        "{}",
        String::from_utf8_lossy(&fresh.stderr)
    );

    let manifest = package.join("manifest.yaml");
    let manifest_text = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        manifest_text.replacen(
            "source_revision: sha256:",
            "source_revision: stale-sha256:",
            1,
        ),
    )
    .unwrap();
    let stale_source = Command::new(binary)
        .args([
            "--root",
            root,
            "validate-package",
            package.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!stale_source.status.success());
    assert!(String::from_utf8_lossy(&stale_source.stdout).contains("PACKAGE_SOURCE_STALE"));
    fs::write(&manifest, manifest_text).unwrap();

    let text = package.join("spreads/001-opening/text.txt");
    fs::write(&text, "Changed package text.\n").unwrap();
    let stale_text = Command::new(binary)
        .args([
            "--root",
            root,
            "validate-package",
            package.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!stale_text.status.success());
    assert!(String::from_utf8_lossy(&stale_text.stdout).contains("PACKAGE_TEXT_STALE"));

    let flow = directory
        .path()
        .join("compendiums/01-magic/01-first/story.flow.yaml");
    let flow_text = fs::read_to_string(&flow).unwrap();
    fs::write(
        &flow,
        flow_text.replacen("role: opening", "role: reveal", 1),
    )
    .unwrap();
    let stale_flow = Command::new(binary)
        .args([
            "--root",
            root,
            "validate-package",
            package.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!stale_flow.status.success());
    assert!(String::from_utf8_lossy(&stale_flow.stdout).contains("PACKAGE_FLOW_STALE"));
    fs::write(&flow, flow_text).unwrap();

    let composition = directory
        .path()
        .join("compendiums/01-magic/01-first/hardcover.composition.yaml");
    let composition_text = fs::read_to_string(&composition).unwrap();
    fs::write(
        &composition,
        composition_text.replacen("density: standard", "density: dense", 1),
    )
    .unwrap();
    let stale_composition = Command::new(binary)
        .args([
            "--root",
            root,
            "validate-package",
            package.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!stale_composition.status.success());
    assert!(
        String::from_utf8_lossy(&stale_composition.stdout).contains("PACKAGE_COMPOSITION_STALE")
    );
}

#[test]
fn package_validation_makes_fragmentation_advisory_strict_or_waived() {
    let directory = package_project();
    fs::write(
        directory.path().join("compositor.toml"),
        format!(
            "{DEFAULT_CONFIG}\n[editorial.paragraph_economy]\nminimum_words = 80\nmax_paragraphs_per_100_words = 12.0\nshort_paragraph_max_words = 12\nmax_consecutive_short_paragraphs = 5\n"
        ),
    )
    .unwrap();
    let story_path = directory
        .path()
        .join("compendiums/01-magic/01-first/story.md");
    let paragraphs = (0..16)
        .map(|ordinal| format!("<!-- paragraph: first-{ordinal} -->\n\nOne small beat is here."))
        .collect::<Vec<_>>()
        .join("\n\n");
    fs::write(
        &story_path,
        format!("---\nid: first\ntitle: First\n---\n<!-- anchor: opening -->\n\n{paragraphs}\n"),
    )
    .unwrap();
    let story = compositor::flow::load_story(&story_path).unwrap();
    let flow_path = directory
        .path()
        .join("compendiums/01-magic/01-first/story.flow.yaml");
    let flow = format!(
        "schema: compositor.dev/story-flow/v1\nstory:\n  id: first\n  source_revision: {}\nspreads:\n  - id: spread-001\n    source:\n      from: {{ type: paragraph, id: first-0 }}\n      through: {{ type: paragraph, id: first-15 }}\n    role: opening\n    energy: 1\n    narrative: {{ purpose: Open the story. }}\n",
        story.source_hash
    );
    fs::write(&flow_path, &flow).unwrap();

    let binary = env!("CARGO_BIN_EXE_compositor");
    let root = directory.path().to_str().unwrap();
    let build = Command::new(binary)
        .args(["--root", root, "build", "magic", "first"])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let package = directory.path().join("output/packages/magic/r01/01-first");

    let advisory = Command::new(binary)
        .args([
            "--root",
            root,
            "validate-package",
            package.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(advisory.status.success());
    assert!(
        String::from_utf8_lossy(&advisory.stdout).contains("PARAGRAPH_ECONOMY_FRAGMENTED"),
        "{}",
        String::from_utf8_lossy(&advisory.stdout)
    );
    let strict = Command::new(binary)
        .args([
            "--root",
            root,
            "validate-package",
            package.to_str().unwrap(),
            "--strict",
        ])
        .output()
        .unwrap();
    assert!(!strict.status.success());
    assert!(String::from_utf8_lossy(&strict.stdout).contains("PARAGRAPH_ECONOMY_FRAGMENTED"));

    fs::write(
        &flow_path,
        format!(
            "{flow}notes:\n  - code: INTENTIONAL_PARAGRAPH_FRAGMENTATION\n    severity: info\n    spread: spread-001\n    message: The deliberate beats support a read-aloud refrain.\n"
        ),
    )
    .unwrap();
    let rebuilt = Command::new(binary)
        .args(["--root", root, "build", "magic", "first"])
        .output()
        .unwrap();
    assert!(
        rebuilt.status.success(),
        "{}",
        String::from_utf8_lossy(&rebuilt.stderr)
    );
    let waived_package = directory.path().join("output/packages/magic/r02/01-first");
    let waived = Command::new(binary)
        .args([
            "--root",
            root,
            "validate-package",
            waived_package.to_str().unwrap(),
            "--strict",
        ])
        .output()
        .unwrap();
    assert!(
        waived.status.success(),
        "{}",
        String::from_utf8_lossy(&waived.stderr)
    );
    assert!(!String::from_utf8_lossy(&waived.stdout).contains("PARAGRAPH_ECONOMY_FRAGMENTED"));
}

#[test]
fn flat_story_sources_are_rejected_after_the_layout_cutover() {
    let directory = project();
    fs::write(
        directory.path().join("compendiums/01-magic/02-flat.md"),
        "---\nid: flat\ntitle: Flat\n---\nThis source is in the old layout.\n",
    )
    .unwrap();
    let config = Config::load(directory.path()).unwrap();
    let error = compositor::discovery::discover(directory.path(), &config).unwrap_err();
    assert!(error
        .to_string()
        .contains("flat story sources are unsupported"));
}
