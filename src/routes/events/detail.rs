//! An event's page and its side panel.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::form::MaintenanceChoice;
use crate::error::AppError;
use crate::i18n::{I18n, Locale};
use crate::middleware::{CsrfToken, OptionalUser};
use crate::models::{Event, EventReview, Kind, Lifecycle, Service, ServiceTag, User, delay_text};
use crate::repositories::{EventRepository, EventReviewRepository, ServiceRepository};
use crate::routes::timeline::{self, TimelineEntry};
use crate::routes::{Frame, members_only, render};
use crate::services::{EventService, can_modify, can_review, sanitize_markdown};
use crate::state::AppState;

pub struct TransitionOption {
    pub value: &'static str,
    pub label: String,
    pub tone: &'static str,
    pub selected: bool,
}

/// What the author had typed when the composer is shown again.
#[derive(Default)]
pub struct Composer {
    pub message: String,
    pub lifecycle: Option<Lifecycle>,
    /// When the next update is due, as written.
    pub next_update: String,
    pub error: Option<String>,
}

impl Composer {
    /// A fresh composer offers again the delay the last update gave.
    fn prefilled(mut self, event: &Event) -> Self {
        if self.error.is_none() {
            self.next_update = event
                .update_every_minutes
                .map(delay_text)
                .unwrap_or_default();
        }
        self
    }
}

/// What a closed event offers whoever publishes: after a maintenance, the
/// announcement of what is new; after an incident, its review to write,
/// correct or remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowUp {
    None,
    Changelog,
    Review,
}

impl FollowUp {
    fn of(event: &Event, user: Option<&User>) -> Self {
        let Some(user) = user else {
            return Self::None;
        };
        if can_review(event, user.role) {
            Self::Review
        } else if user.role.can_publish()
            && event.kind == Kind::Maintenance
            && event.lifecycle == Some(Lifecycle::Completed)
        {
            Self::Changelog
        } else {
            Self::None
        }
    }

    pub fn is_changelog(self) -> bool {
        self == Self::Changelog
    }

    pub fn is_review(self) -> bool {
        self == Self::Review
    }
}

/// What the review form holds when it is shown again.
#[derive(Default)]
pub struct ReviewForm {
    pub body: String,
    pub error: Option<String>,
}

impl ReviewForm {
    /// A fresh form holds the review as written, to correct it.
    fn prefilled(mut self, view: &EventView) -> Self {
        if self.error.is_none() {
            self.body = view
                .review
                .as_ref()
                .map(|review| review.body.clone())
                .unwrap_or_default();
        }
        self
    }
}

#[derive(Template)]
#[template(path = "events/detail.html")]
pub(super) struct EventDetailTemplate {
    frame: Frame,
    view: EventView,
    can_edit: bool,
    follow_up: FollowUp,
    transitions: Vec<TransitionOption>,
    revert_label: Option<String>,
    published_notice: bool,
    composer: Composer,
    review_form: ReviewForm,
    i18n: I18n,
}

#[derive(Template)]
#[template(path = "events/drawer_content.html")]
pub(super) struct DrawerTemplate {
    view: EventView,
    can_edit: bool,
    csrf_token: String,
    transitions: Vec<TransitionOption>,
    /// Undoing a step is offered on the event's page only.
    revert_label: Option<String>,
    /// The next step after the event, done on its page.
    follow_up: FollowUp,
    /// The service whose panel this one was opened from, to go back to.
    back_to: Option<(i64, String)>,
    composer: Composer,
    i18n: I18n,
}

/// An event as its page and its side panel show it.
pub struct EventView {
    pub event: Event,
    pub services: Vec<Service>,
    /// The maintenance this announcement follows.
    pub follows: Option<MaintenanceChoice>,
    pub state: Option<StateLine>,
    /// An announcement reads as a text, its updates after it.
    pub article: Option<TimelineEntry>,
    /// A resolved incident reads its review first, its timeline after it.
    pub review: Option<ReviewView>,
    pub timeline: Vec<TimelineEntry>,
}

/// The review of a resolved incident, dated and, for members, signed.
pub struct ReviewView {
    pub time: String,
    pub iso: String,
    pub author: Option<String>,
    pub html: String,
    /// As its author wrote it, for the form that corrects it.
    pub body: String,
}

impl ReviewView {
    fn new(review: EventReview, user: Option<&User>, i18n: &I18n) -> Self {
        Self {
            time: i18n.format_datetime(&review.published_at),
            iso: review.published_at.to_rfc3339(),
            author: user.map(|_| review.author_name),
            html: sanitize_markdown(&review.body),
            body: review.body,
        }
    }
}

impl EventView {
    pub fn service_tags(&self) -> Vec<ServiceTag> {
        self.services.iter().map(ServiceTag::from).collect()
    }

    /// The heading of an announcement's text: its category.
    pub fn article_title(&self, i18n: &I18n) -> String {
        self.event
            .qualifier(i18n)
            .unwrap_or_else(|| i18n.t(self.event.kind.i18n_key()).to_string())
    }

    /// A row of facts is worth drawing when there is one.
    pub fn has_facts(&self) -> bool {
        self.state.is_some() || !self.services.is_empty() || self.follows.is_some()
    }
}

/// Where the event stands, in words: the state and how long it has held.
pub struct StateLine {
    pub tone: &'static str,
    pub label: String,
    pub detail: Option<String>,
    /// How long ago the last update was posted on an open incident, and
    /// when the next one is due.
    pub last_news: Option<String>,
}

async fn load_view(
    state: &AppState,
    id: i64,
    user: Option<&User>,
    i18n: &I18n,
) -> Result<EventView, AppError> {
    let ews = EventService::find_with_services(&state.pool, id).await?;
    let updates = EventRepository::list_updates(&state.pool, id).await?;
    let last_posted = updates.iter().map(|update| update.created_at).max();
    let author = match user {
        Some(_) => Some(author_name(state, ews.event.author_id, i18n).await?),
        None => None,
    };
    let follows = match ews.event.follows_event_id {
        Some(id) => EventRepository::find_by_id(&state.pool, id)
            .await?
            .map(|m| {
                MaintenanceChoice::new(id, m.title, &m.ended_at.unwrap_or(m.updated_at), i18n)
            }),
        None => None,
    };
    let mut timeline = timeline::build(
        &ews.event,
        sanitize_markdown(&ews.event.description),
        author,
        updates,
        user,
        i18n,
    );
    let article = (ews.event.kind == Kind::Publication)
        .then(|| timeline.iter().position(|entry| entry.update_id.is_none()))
        .flatten()
        .map(|opening| timeline.remove(opening));
    let review = load_review(state, &ews.event, user, i18n).await?;
    Ok(EventView {
        follows,
        article,
        review,
        state: state_line(&ews.event, last_posted, i18n),
        event: ews.event,
        services: ews.services,
        timeline,
    })
}

/// The review of a resolved incident; none for any other event, nor for an
/// incident an administrator opened again.
async fn load_review(
    state: &AppState,
    event: &Event,
    user: Option<&User>,
    i18n: &I18n,
) -> Result<Option<ReviewView>, AppError> {
    if !event.takes_review() {
        return Ok(None);
    }
    let review = EventReviewRepository::find(&state.pool, event.id).await?;
    Ok(review.map(|review| ReviewView::new(review, user, i18n)))
}

async fn author_name(state: &AppState, author_id: i64, i18n: &I18n) -> Result<String, AppError> {
    Ok(
        crate::repositories::UserRepository::find_by_id(&state.pool, author_id)
            .await?
            .map_or_else(
                || i18n.t("date.unknown_author").to_string(),
                |u| u.display_name,
            ),
    )
}

fn state_line(event: &Event, last_posted: Option<DateTime<Utc>>, i18n: &I18n) -> Option<StateLine> {
    let lifecycle = event.lifecycle?;
    let last_ago = last_posted
        .filter(|_| event.kind == Kind::Incident && lifecycle.is_active())
        .map(|at| i18n.format_ago(&at));
    let last_news = event
        .next_update_text(last_ago.as_deref(), i18n)
        .or_else(|| last_ago.map(|ago| i18n.tf("events.last_news", &[("ago", &ago)])));
    let detail = match lifecycle {
        Lifecycle::Scheduled => i18n.format_countdown(event.countdown()),
        Lifecycle::Cancelled => None,
        open if open.is_active() => event.elapsed_text(i18n),
        _ => event.duration().map(|parts| {
            i18n.tf(
                "events.lasted",
                &[("duration", &i18n.format_duration(&parts))],
            )
        }),
    };
    Some(StateLine {
        tone: event.tone().as_str(),
        label: i18n.t(lifecycle.label_key(event.kind)).to_string(),
        detail,
        last_news,
    })
}

fn transition_options(
    event: &Event,
    selected: Option<Lifecycle>,
    i18n: &I18n,
) -> Vec<TransitionOption> {
    let Some(current) = event.lifecycle else {
        return Vec::new();
    };
    event
        .kind
        .allowed_transitions(current)
        .iter()
        .map(|next| TransitionOption {
            value: next.as_str(),
            label: i18n.t(next.label_key(event.kind)).to_string(),
            tone: event.tone_at(*next),
            selected: selected == Some(*next),
        })
        .collect()
}

#[derive(Deserialize)]
pub struct DetailQuery {
    #[serde(default)]
    published: Option<String>,
}

pub async fn detail(
    OptionalUser(user): OptionalUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<DetailQuery>,
    headers: HeaderMap,
    csrf_token: CsrfToken,
    Locale(i18n): Locale,
) -> Result<Response, AppError> {
    if let Some(redirect) = members_only(&state, user.as_ref(), &headers) {
        return Ok(redirect);
    }
    let page = detail_page(
        &state,
        id,
        user.as_ref(),
        csrf_token.0,
        i18n,
        Composer::default(),
        ReviewForm::default(),
    )
    .await?;
    render(&EventDetailTemplate {
        published_notice: query.published.is_some() && page.frame.can_publish,
        ..page
    })
}

impl DrawerTemplate {
    /// "Back to Mail", the link's name for screen readers.
    fn back_label(&self, service: &str) -> String {
        self.i18n.tf("events.back_to_service", &[("name", service)])
    }
}

/// The event's page, with what the author had typed in the form that was
/// refused, `composer` or `review_form`.
pub(super) async fn detail_page(
    state: &AppState,
    id: i64,
    user: Option<&User>,
    csrf_token: String,
    i18n: I18n,
    composer: Composer,
    review_form: ReviewForm,
) -> Result<EventDetailTemplate, AppError> {
    let view = load_view(state, id, user, &i18n).await?;
    let composer = composer.prefilled(&view.event);
    let review_form = review_form.prefilled(&view);
    let can_edit = user.is_some_and(|u| can_modify(&view.event, u.role));
    Ok(EventDetailTemplate {
        follow_up: FollowUp::of(&view.event, user),
        frame: Frame::load(&state.pool, user, csrf_token, &i18n).await?,
        transitions: transition_options(&view.event, composer.lifecycle, &i18n),
        revert_label: can_edit.then(|| revert_label(&view.event, &i18n)).flatten(),
        published_notice: false,
        can_edit,
        view,
        composer,
        review_form,
        i18n,
    })
}

/// "Back to Investigating", when the last step can be undone; offered only
/// to whoever may change the event, as the undo itself checks.
fn revert_label(event: &Event, i18n: &I18n) -> Option<String> {
    event
        .previous_lifecycle_key()
        .map(|key| i18n.tf("events.revert_to", &[("state", i18n.t(key))]))
}

pub(super) async fn drawer_page(
    state: &AppState,
    id: i64,
    user: Option<&User>,
    csrf_token: String,
    i18n: I18n,
    composer: Composer,
) -> Result<DrawerTemplate, AppError> {
    let view = load_view(state, id, user, &i18n).await?;
    let composer = composer.prefilled(&view.event);
    let can_edit = user.is_some_and(|u| can_modify(&view.event, u.role));
    Ok(DrawerTemplate {
        transitions: transition_options(&view.event, composer.lifecycle, &i18n),
        revert_label: None,
        follow_up: FollowUp::of(&view.event, user),
        back_to: None,
        can_edit,
        csrf_token,
        view,
        composer,
        i18n,
    })
}

#[derive(Deserialize)]
pub struct DrawerQuery {
    /// The service whose panel the event was opened from.
    service: Option<i64>,
}

pub async fn drawer_content(
    OptionalUser(user): OptionalUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<DrawerQuery>,
    headers: HeaderMap,
    csrf_token: CsrfToken,
    Locale(i18n): Locale,
) -> Result<Response, AppError> {
    if let Some(redirect) = members_only(&state, user.as_ref(), &headers) {
        return Ok(redirect);
    }
    let mut page = drawer_page(
        &state,
        id,
        user.as_ref(),
        csrf_token.0,
        i18n,
        Composer::default(),
    )
    .await?;
    if let Some(service_id) = query.service {
        page.back_to = ServiceRepository::find_by_id(&state.pool, service_id)
            .await?
            .map(|service| (service.id, service.name));
    }
    render(&page)
}
