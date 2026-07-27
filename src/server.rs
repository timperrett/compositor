//! Local, server-rendered artwork review UI.

use crate::art_dashboard::{self, Dashboard, DashboardEntry};
use crate::assets::AssetStatus;
use crate::config::Config;
use crate::AppError;
use askama::Template;
use axum::body::Body;
use axum::extract::{Path as RoutePath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

#[cfg(unix)]
use std::future::Future;
#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};

const LOCK_PATH: &str = "output/locks/compositor-server.lock";

#[derive(Clone)]
struct ServerState {
    root: PathBuf,
    config: Config,
    csrf_token: String,
    mutations: Arc<Mutex<()>>,
}

#[derive(Deserialize)]
struct SelectForm {
    csrf: String,
    candidate_id: String,
}

#[derive(Deserialize)]
struct CsrfForm {
    csrf: String,
}

struct ProjectLock {
    path: PathBuf,
}

impl ProjectLock {
    fn acquire(root: &Path) -> Result<Self, AppError> {
        let path = root.join(LOCK_PATH);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().write(true).create_new(true).open(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                AppError::command(format!(
                    "cannot start the art server because lock `{}` already exists; stop the existing server or, after confirming none is running, remove the stale lock",
                    path.display()
                ))
            } else {
                AppError::Io(error)
            }
        })?;
        use std::io::Write;
        writeln!(
            file,
            "pid={}\nstarted_unix_seconds={}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        )?;
        Ok(Self { path })
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(crate) fn serve(root: &Path, config: &Config, port: u16) -> Result<(), AppError> {
    let lock = ProjectLock::acquire(root)?;
    let state = ServerState {
        root: root.to_path_buf(),
        config: config.clone(),
        csrf_token: csrf_token()?,
        mutations: Arc::new(Mutex::new(())),
    };
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    println!("Compositor art review server: http://{address}");
    let runtime = tokio::runtime::Runtime::new().map_err(AppError::Io)?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(AppError::Io)?;
        #[cfg(unix)]
        let shutdown = shutdown_signal()?;
        #[cfg(not(unix))]
        let shutdown = shutdown_signal();
        let _lock = lock;
        axum::serve(listener, router(state))
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(AppError::Io)
    })
}

fn router(state: ServerState) -> Router {
    Router::new()
        .route("/", get(dashboard))
        .route("/art/{art_id}/select", post(select))
        .route("/art/{art_id}/review", post(review))
        .route("/art/{art_id}/reject", post(reject))
        .route("/art/{art_id}/approve", post(approve))
        .route("/art/{art_id}/unplace", post(unplace))
        .route(
            "/art/{art_id}/candidate/{candidate_id}",
            get(candidate_image),
        )
        .route("/art/{art_id}/approved", get(approved_image))
        .with_state(state)
}

#[cfg(unix)]
fn shutdown_signal() -> Result<impl Future<Output = ()>, AppError> {
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    Ok(async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
            _ = hangup.recv() => {}
        }
    })
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn dashboard(State(state): State<ServerState>) -> Result<Html<String>, ServerError> {
    let dashboard = art_dashboard::analyze(&state.root, &state.config)?;
    render(DashboardTemplate::from_dashboard(
        &dashboard,
        &state.csrf_token,
    ))
}

async fn select(
    State(state): State<ServerState>,
    RoutePath(art_id): RoutePath<String>,
    headers: HeaderMap,
    Form(form): Form<SelectForm>,
) -> Result<Response, ServerError> {
    validate_csrf(&state, &form.csrf)?;
    let _guard = state.mutations.lock().await;
    crate::art_workflow::select(
        &state.root,
        &state.config,
        &art_id,
        &form.candidate_id,
        None,
    )?;
    action_response(&state, &art_id, &headers)
}

async fn review(
    State(state): State<ServerState>,
    RoutePath(art_id): RoutePath<String>,
    headers: HeaderMap,
    Form(form): Form<CsrfForm>,
) -> Result<Response, ServerError> {
    validate_csrf(&state, &form.csrf)?;
    let _guard = state.mutations.lock().await;
    crate::art_workflow::review(&state.root, &state.config, &art_id)?;
    action_response(&state, &art_id, &headers)
}

async fn reject(
    State(state): State<ServerState>,
    RoutePath(art_id): RoutePath<String>,
    headers: HeaderMap,
    Form(form): Form<CsrfForm>,
) -> Result<Response, ServerError> {
    validate_csrf(&state, &form.csrf)?;
    let _guard = state.mutations.lock().await;
    crate::art_workflow::reject(&state.root, &state.config, &art_id)?;
    action_response(&state, &art_id, &headers)
}

async fn approve(
    State(state): State<ServerState>,
    RoutePath(art_id): RoutePath<String>,
    headers: HeaderMap,
    Form(form): Form<CsrfForm>,
) -> Result<Response, ServerError> {
    validate_csrf(&state, &form.csrf)?;
    let _guard = state.mutations.lock().await;
    crate::art_workflow::approve(&state.root, &state.config, &art_id)?;
    action_response(&state, &art_id, &headers)
}

async fn unplace(
    State(state): State<ServerState>,
    RoutePath(art_id): RoutePath<String>,
    headers: HeaderMap,
    Form(form): Form<CsrfForm>,
) -> Result<Response, ServerError> {
    validate_csrf(&state, &form.csrf)?;
    let _guard = state.mutations.lock().await;
    crate::art_workflow::unplace(&state.root, &state.config, &art_id)?;
    action_response(&state, &art_id, &headers)
}

fn action_response(
    state: &ServerState,
    art_id: &str,
    headers: &HeaderMap,
) -> Result<Response, ServerError> {
    if headers
        .get("x-compositor-dashboard")
        .is_some_and(|value| value == "1")
    {
        return render(CardTemplate {
            entry: card_for_art(state, art_id)?,
            csrf: state.csrf_token.clone(),
        })
        .map(IntoResponse::into_response);
    }
    Ok(Redirect::to("/").into_response())
}

fn card_for_art(state: &ServerState, art_id: &str) -> Result<DashboardCard, ServerError> {
    let dashboard = art_dashboard::analyze(&state.root, &state.config)?;
    let entry = dashboard
        .entries
        .iter()
        .find(|entry| entry.art_id == art_id)
        .ok_or_else(|| ServerError::not_found("artwork record not found"))?;
    let story = entry
        .story_id
        .clone()
        .unwrap_or_else(|| "Unplaced/orphan".into());
    Ok(DashboardCard::from_entry(entry, story))
}

async fn candidate_image(
    State(state): State<ServerState>,
    RoutePath((art_id, candidate_id)): RoutePath<(String, String)>,
) -> Result<Response, ServerError> {
    let brief = crate::art_brief::load(&state.root, &art_id)?
        .ok_or_else(|| ServerError::not_found("artwork brief not found"))?;
    let candidate = brief
        .candidates
        .iter()
        .find(|candidate| candidate.id == candidate_id)
        .ok_or_else(|| ServerError::not_found("candidate not found"))?;
    image_response(&state.root, &candidate.file)
}

async fn approved_image(
    State(state): State<ServerState>,
    RoutePath(art_id): RoutePath<String>,
) -> Result<Response, ServerError> {
    let registry = crate::assets::load(&state.root)?
        .ok_or_else(|| ServerError::not_found("asset registry not found"))?;
    let file = crate::assets::record(&registry, &art_id)
        .and_then(|asset| asset.approved.as_ref())
        .map(|approved| approved.file.clone())
        .ok_or_else(|| ServerError::not_found("approved artwork not found"))?;
    image_response(&state.root, &file)
}

fn image_response(root: &Path, file: &str) -> Result<Response, ServerError> {
    let root = root.canonicalize()?;
    let path = root
        .join(file)
        .canonicalize()
        .map_err(|_| ServerError::not_found("image not found"))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(ServerError::not_found("image not found"));
    }
    let bytes = fs::read(&path)?;
    let content_type = match path.extension().and_then(|value| value.to_str()) {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("png") => "image/png",
        _ => return Err(ServerError::not_found("unsupported image type")),
    };
    Ok(([(header::CONTENT_TYPE, content_type)], Body::from(bytes)).into_response())
}

fn render(template: impl Template) -> Result<Html<String>, ServerError> {
    template
        .render()
        .map(Html)
        .map_err(|error| ServerError::internal(format!("could not render page: {error}")))
}

fn csrf_token() -> Result<String, AppError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| AppError::command(format!("could not create CSRF token: {error}")))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn validate_csrf(state: &ServerState, csrf: &str) -> Result<(), ServerError> {
    if csrf == state.csrf_token {
        Ok(())
    } else {
        Err(ServerError::forbidden("CSRF token is invalid"))
    }
}

#[derive(Debug)]
struct ServerError {
    status: StatusCode,
    message: String,
}

impl ServerError {
    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl From<AppError> for ServerError {
    fn from(error: AppError) -> Self {
        Self::internal(error.to_string())
    }
}

impl From<std::io::Error> for ServerError {
    fn from(error: std::io::Error) -> Self {
        Self::internal(error.to_string())
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let body = ErrorTemplate {
            message: self.message,
        }
        .render()
        .unwrap_or_else(|_| {
            "<!doctype html><title>Compositor</title><p>Request failed.</p>".into()
        });
        (self.status, Html(body)).into_response()
    }
}

#[derive(Template)]
#[template(path = "server/error.html")]
struct ErrorTemplate {
    message: String,
}

#[derive(Template)]
#[template(path = "server/dashboard.html")]
struct DashboardTemplate {
    compendiums: Vec<String>,
    stories: Vec<String>,
    readiness_labels: Vec<String>,
    story_groups: Vec<DashboardStoryGroup>,
    csrf: String,
}

#[derive(Template)]
#[template(path = "server/card.html")]
struct CardTemplate {
    entry: DashboardCard,
    csrf: String,
}

struct DashboardStoryGroup {
    name: String,
    entries: Vec<DashboardCard>,
}

struct DashboardCard {
    art_id: String,
    compendium: String,
    story: String,
    placement: String,
    required: bool,
    lifecycle: String,
    readiness: String,
    blocker: bool,
    candidate_count: usize,
    candidates: Vec<DashboardCandidate>,
    can_select: bool,
    has_selected_candidate: bool,
    selected_candidate_id: String,
    selected_candidate_url: String,
    approved: bool,
    approved_url: String,
    can_review: bool,
    can_approve: bool,
    can_unplace: bool,
    opener_art: bool,
    guidance: String,
    card_class: String,
}

struct DashboardCandidate {
    id: String,
    url: String,
}

impl DashboardTemplate {
    fn from_dashboard(dashboard: &Dashboard, csrf: &str) -> Self {
        let compendiums = dashboard
            .entries
            .iter()
            .filter_map(|entry| entry.compendium_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let stories = dashboard
            .entries
            .iter()
            .filter_map(|entry| entry.story_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let readiness_labels = dashboard
            .entries
            .iter()
            .map(|entry| entry.readiness.label.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut grouped = BTreeMap::<String, Vec<DashboardCard>>::new();
        for entry in &dashboard.entries {
            let story = entry
                .story_id
                .clone()
                .unwrap_or_else(|| "Unplaced/orphan".into());
            grouped
                .entry(story.clone())
                .or_default()
                .push(DashboardCard::from_entry(entry, story));
        }
        Self {
            compendiums,
            stories,
            readiness_labels,
            story_groups: grouped
                .into_iter()
                .map(|(name, entries)| DashboardStoryGroup { name, entries })
                .collect(),
            csrf: csrf.into(),
        }
    }
}

impl DashboardCard {
    fn from_entry(entry: &DashboardEntry, story: String) -> Self {
        let compendium = entry
            .compendium_id
            .clone()
            .unwrap_or_else(|| "Unplaced/orphan".into());
        let candidates = entry
            .candidates
            .iter()
            .map(|candidate| DashboardCandidate {
                id: candidate.id.clone(),
                url: format!("/art/{}/candidate/{}", entry.art_id, candidate.id),
            })
            .collect::<Vec<_>>();
        let card_class = if !entry.required {
            "orphan"
        } else if entry.readiness.default_policy_ready {
            "ready"
        } else {
            "blocked"
        };
        let lifecycle = status_name(entry.registry_status);
        let selected_candidate = entry.selected_candidate.as_deref().and_then(|selected_id| {
            candidates
                .iter()
                .find(|candidate| candidate.id == selected_id)
        });
        Self {
            art_id: entry.art_id.clone(),
            compendium,
            story,
            placement: entry.placement.clone(),
            required: entry.required,
            lifecycle: lifecycle.into(),
            readiness: entry.readiness.label.clone(),
            blocker: entry.required && !entry.readiness.default_policy_ready,
            candidate_count: entry.candidate_count,
            can_select: entry.required && lifecycle == "requested" && !candidates.is_empty(),
            has_selected_candidate: selected_candidate.is_some(),
            selected_candidate_id: selected_candidate
                .map(|candidate| candidate.id.clone())
                .unwrap_or_default(),
            selected_candidate_url: selected_candidate
                .map(|candidate| candidate.url.clone())
                .unwrap_or_default(),
            candidates,
            approved: entry.approved_artwork.is_some(),
            approved_url: format!("/art/{}/approved", entry.art_id),
            can_review: entry.required && lifecycle == "draft",
            can_approve: entry.required && lifecycle == "review",
            can_unplace: entry.can_unplace,
            opener_art: entry.required && !entry.can_unplace,
            guidance: entry.readiness.next_action.clone(),
            card_class: card_class.into(),
        }
    }
}

fn status_name(status: Option<AssetStatus>) -> &'static str {
    match status {
        None => "unregistered",
        Some(AssetStatus::Requested) => "requested",
        Some(AssetStatus::Draft) => "draft",
        Some(AssetStatus::Review) => "review",
        Some(AssetStatus::Approved) => "approved",
        Some(AssetStatus::Rejected) => "rejected",
        Some(AssetStatus::Superseded) => "superseded",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_refuses_an_existing_file_and_removes_on_drop() {
        let directory = tempfile::tempdir().unwrap();
        let lock = ProjectLock::acquire(directory.path()).unwrap();
        assert!(ProjectLock::acquire(directory.path()).is_err());
        let path = lock.path.clone();
        drop(lock);
        assert!(!path.exists());
    }

    #[test]
    fn dashboard_renders_one_relevant_next_state_control_per_card() {
        let template = DashboardTemplate {
            compendiums: vec![],
            stories: vec![],
            readiness_labels: vec![],
            csrf: "csrf".into(),
            story_groups: vec![DashboardStoryGroup {
                name: "story".into(),
                entries: vec![
                    test_card("requested", true, false, false),
                    test_card("draft", false, true, false),
                    test_card("review", false, false, true),
                    test_card("approved", false, false, false),
                ],
            }],
        };

        let rendered = template.render().unwrap();
        assert_eq!(rendered.matches("Select candidate</button>").count(), 1);
        assert_eq!(rendered.matches("Send to review</button>").count(), 1);
        assert_eq!(rendered.matches("Approve</button>").count(), 1);
        assert_eq!(rendered.matches("Reject</button>").count(), 1);
        assert_eq!(rendered.matches("Not needed</button>").count(), 4);
        assert!(rendered.contains("/unplace"));
        assert!(rendered.contains("Remove requested from its Composition placement?"));
        assert!(rendered.contains("type=\"radio\""));
        assert!(rendered.contains("required"));
        assert!(!rendered.contains("name=\"feedback\""));
        assert!(!rendered.contains("Review artwork"));
        assert_eq!(rendered.matches("<form ").count(), 8);
        assert!(rendered.contains("history.replaceState"));
        assert!(rendered.contains("X-Compositor-Dashboard"));
        assert!(rendered.contains("application/x-www-form-urlencoded"));
        assert!(!rendered.contains("Reject review?"));

        let card = CardTemplate {
            entry: test_card("review", false, false, true),
            csrf: "csrf".into(),
        }
        .render()
        .unwrap();
        assert!(card.trim_start().starts_with("<article"));
        assert_eq!(card.matches("<form ").count(), 3);

        let mut opener = test_card("opener", false, false, false);
        opener.can_unplace = false;
        opener.opener_art = true;
        let opener = CardTemplate {
            entry: opener,
            csrf: "csrf".into(),
        }
        .render()
        .unwrap();
        assert!(!opener.contains("/unplace"));
        assert!(opener.contains("Opener artwork is required"));

        let mut orphan = test_card("orphan", false, false, false);
        orphan.required = false;
        orphan.can_unplace = false;
        orphan.opener_art = false;
        orphan.readiness = "unplaced/orphan".into();
        let orphan = CardTemplate {
            entry: orphan,
            csrf: "csrf".into(),
        }
        .render()
        .unwrap();
        assert!(!orphan.contains("<form "));
        assert!(orphan.contains("No action required"));
    }

    fn test_card(
        art_id: &str,
        can_select: bool,
        can_review: bool,
        can_approve: bool,
    ) -> DashboardCard {
        DashboardCard {
            art_id: art_id.into(),
            compendium: "compendium".into(),
            story: "story".into(),
            placement: "opener".into(),
            required: true,
            lifecycle: art_id.into(),
            readiness: "ready".into(),
            blocker: false,
            candidate_count: 1,
            candidates: vec![DashboardCandidate {
                id: "candidate".into(),
                url: "/candidate".into(),
            }],
            can_select,
            has_selected_candidate: can_review || can_approve,
            selected_candidate_id: "candidate".into(),
            selected_candidate_url: "/candidate".into(),
            approved: art_id == "approved",
            approved_url: "/approved".into(),
            can_review,
            can_approve,
            can_unplace: true,
            opener_art: false,
            guidance: "No action required".into(),
            card_class: "ready".into(),
        }
    }
}
