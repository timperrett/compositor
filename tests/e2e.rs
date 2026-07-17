use compositor::build;
use compositor::config::{Config, DEFAULT_CONFIG};
use compositor::model::ChangeKind;
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
    assert!(String::from_utf8(build.stdout)
        .unwrap()
        .contains("\"command\": \"build\""));
}
