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
    fs::write(compendium.join("01-story.md"), "---\nid: story\ntitle: Story\n---\n<!-- anchor: opening -->\nOnce upon a time.\n\n---\nA second unit stays the same.\n").unwrap();
    directory
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
        directory.path().join("compendiums/01-magic/01-story.md"),
        format!("---\nid: story\ntitle: Story\n---\n{units}\n"),
    )
    .unwrap();
}

fn replace_story_with_text(directory: &tempfile::TempDir, text: &str) {
    fs::write(
        directory.path().join("compendiums/01-magic/01-story.md"),
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
        directory.path().join("compendiums/01-magic/01-story.md"),
        "---\nid: story\ntitle: Story\n---\n<!-- anchor: opening -->\n# A **bold** beginning\n\nA [linked](https://example.com) paragraph.\n\n- First item\n- Second item\n\n---\n\n> A closing quotation.\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("compendiums/01-magic/02-second.md"),
        "---\nid: second\ntitle: Second Story\n---\nSecond body.\n",
    )
    .unwrap();
    let config = Config::load(directory.path()).unwrap();

    build::build(directory.path(), &config, None).unwrap();

    let story = fs::read_to_string(directory.path().join("output/text/story.txt")).unwrap();
    assert_eq!(
        story,
        "Story\n\nA bold beginning\n\nA linked paragraph.\n\n• First item\n• Second item\n\nA closing quotation.\n"
    );
    assert!(!story.contains("<!--"));
    assert!(!story.contains("**"));
    assert!(!story.contains("[linked]("));
    let compendium = fs::read_to_string(directory.path().join("output/text/magic.txt")).unwrap();
    assert_eq!(
        compendium,
        format!("{story}\n\nSecond Story\n\nSecond body.\n")
    );
    assert!(directory.path().join("output/text/second.txt").is_file());
}

#[test]
fn unchanged_build_restores_missing_plain_text_export_and_source_edits_refresh_it() {
    let directory = project();
    let config = Config::load(directory.path()).unwrap();
    build::build(directory.path(), &config, None).unwrap();
    let source = directory.path().join("compendiums/01-magic/01-story.md");
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
    let story = directory.path().join("compendiums/01-magic/01-story.md");
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
    let story = directory.path().join("compendiums/01-magic/01-story.md");
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
        directory.path().join("compendiums/01-magic/01-story.md"),
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
fn selected_art_brief_candidate_is_promoted_and_used_in_proof() {
    let directory = project();
    fs::write(
        directory.path().join("compendiums/01-magic/01-story.md"),
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
        "schema_version: 1\nart_id: reveal\nsource:\n  story_id: story\n  unit_ids: [reveal]\n  requirement_revision: 1\ngeneration:\n  prompt: A moonlit library.\ncandidates:\n  - id: a\n    file: assets/drafts/reveal/r01/candidate-a.png\nselection:\n  candidate_id: a\n",
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
    assert!(String::from_utf8(brief.stdout)
        .unwrap()
        .contains("art/briefs/reveal.yaml"));

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
    fs::write(
        compendium.join("01-story.md"),
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
