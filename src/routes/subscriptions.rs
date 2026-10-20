//! Following the page: the "Get notified" page with its email card, the
//! posts of that card, and the two pages the emails link to.
//!
//! A visitor's request is answered the same way whatever the address, and
//! the work is done after the answer. The link pages need no session: the
//! token in their address is the proof, and a mail client posts to the
//! unsubscribe address without one (RFC 8058).

use std::net::SocketAddr;
use std::sync::Arc;

use askama::Template;
use axum::Extension;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use tower_sessions::Session;

use super::auth::{instance_title, request_ip};
use super::dashboard::origin;
use super::password_reset::link_base;
use super::{Frame, members_only, render};
use crate::clock;
use crate::db::DbPool;
use crate::error::AppError;
use crate::i18n::{I18n, Locale};
use crate::middleware::csrf::form_token;
use crate::middleware::headers::no_store;
use crate::middleware::{AuthUser, CsrfToken, FormCsrfToken, HtmlForm, OptionalUser};
use crate::models::{Choices, JournalKind, Subscriber, User};
use crate::repositories::{NewJournalEntry, SubscriberRepository, UserRepository};
use crate::services::{
    AuthService, ConfirmEmail, Notifier, RequestCheck, SubscribeRequest, SubscriptionService,
};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "subscribe.html")]
struct SubscribeTemplate {
    frame: Frame,
    feed_url: String,
    feed_readable: bool,
    email: Option<EmailCard>,
    i18n: I18n,
}

/// The email card of the page: a visitor's form, the word that a link left
/// for their address, or a member's own subscription.
struct EmailCard {
    step: Step,
    address: String,
    choices: Choices,
    /// What was wrong with the form, in words, and where.
    error: Option<(Spot, String)>,
    /// What was just done, in words: a link sent, choices saved.
    notice: Option<String>,
    /// Since when a member receives the news, in words.
    since: Option<String>,
}

/// The part of the form a mistake is about.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Spot {
    Address,
    Boxes,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Form,
    Asked,
    Member,
}

impl EmailCard {
    fn form(address: &str, choices: Choices, error: Option<(Spot, String)>) -> Self {
        Self {
            step: Step::Form,
            address: address.to_string(),
            choices,
            error,
            notice: None,
            since: None,
        }
    }

    fn is_form(&self) -> bool {
        self.step == Step::Form
    }

    fn address_wrong(&self) -> bool {
        matches!(self.error, Some((Spot::Address, _)))
    }

    fn boxes_wrong(&self) -> bool {
        matches!(self.error, Some((Spot::Boxes, _)))
    }

    fn error_text(&self) -> Option<&str> {
        self.error.as_ref().map(|(_, text)| text.as_str())
    }

    fn is_asked(&self) -> bool {
        self.step == Step::Asked
    }

    fn subscribed(&self) -> bool {
        self.since.is_some()
    }
}

#[derive(Deserialize)]
pub struct PageQuery {
    #[serde(default)]
    done: String,
}

#[derive(Deserialize)]
pub struct VisitorForm {
    #[serde(default)]
    email: String,
    on_incidents: Option<String>,
    on_maintenances: Option<String>,
    on_publications: Option<String>,
}

#[derive(Deserialize)]
pub struct MemberForm {
    #[serde(default)]
    action: String,
    on_incidents: Option<String>,
    on_maintenances: Option<String>,
    on_publications: Option<String>,
}

#[derive(Deserialize)]
pub struct TokenQuery {
    #[serde(default)]
    token: String,
}

impl VisitorForm {
    fn choices(&self) -> Choices {
        Choices {
            incidents: self.on_incidents.is_some(),
            maintenances: self.on_maintenances.is_some(),
            publications: self.on_publications.is_some(),
        }
    }
}

impl MemberForm {
    fn choices(&self) -> Choices {
        Choices {
            incidents: self.on_incidents.is_some(),
            maintenances: self.on_maintenances.is_some(),
            publications: self.on_publications.is_some(),
        }
    }
}

/// Visitors may subscribe: the page is open to everyone and an
/// administrator opened it to their subscriptions.
async fn visitors_welcome(state: &AppState) -> Result<bool, AppError> {
    Ok(state.is_public_mode() && SubscriptionService::visitors_open(&state.pool).await?)
}

/// How to follow updates without opening the page: by email when the page
/// can send it, the feed, a team tool, or the page itself.
pub async fn page(
    OptionalUser(user): OptionalUser,
    State(state): State<AppState>,
    headers: HeaderMap,
    session: Session,
    csrf_token: CsrfToken,
    Locale(i18n): Locale,
    Query(query): Query<PageQuery>,
) -> Result<Response, AppError> {
    if let Some(redirect) = members_only(&state, user.as_ref(), &headers) {
        return Ok(redirect);
    }
    let card = match &user {
        Some(user) => member_card(&state, user, &query.done, &i18n).await?,
        None => visitor_card(&state).await?,
    };
    // A visitor's form needs a session to post; nobody else is given one.
    let csrf_token = match &card {
        Some(card) if card.is_form() => form_token(&session).await?,
        _ => csrf_token.0,
    };
    render_page(&state, user.as_ref(), &headers, csrf_token, card, i18n).await
}

async fn visitor_card(state: &AppState) -> Result<Option<EmailCard>, AppError> {
    if link_base(state).await?.is_none() || !visitors_welcome(state).await? {
        return Ok(None);
    }
    Ok(Some(EmailCard::form("", Choices::EVERYTHING, None)))
}

async fn member_card(
    state: &AppState,
    user: &User,
    done: &str,
    i18n: &I18n,
) -> Result<Option<EmailCard>, AppError> {
    if link_base(state).await?.is_none() {
        return Ok(None);
    }
    let subscription = SubscriberRepository::find_for_member(&state.pool, user.id).await?;
    let notice = match done {
        "saved" => Some(i18n.t("subscribe.saved").to_string()),
        "stopped" => Some(i18n.t("subscribe.stopped").to_string()),
        _ => None,
    };
    Ok(Some(EmailCard {
        step: Step::Member,
        address: user.email.clone(),
        choices: subscription
            .as_ref()
            .map_or(Choices::EVERYTHING, |s| s.choices),
        error: None,
        notice,
        since: subscription.map(|s| {
            let date = i18n.format_date_long(&clock::local_date(&s.created_at));
            i18n.tf("subscribe.since", &[("date", &date)])
        }),
    }))
}

async fn render_page(
    state: &AppState,
    user: Option<&User>,
    headers: &HeaderMap,
    csrf_token: String,
    email: Option<EmailCard>,
    i18n: I18n,
) -> Result<Response, AppError> {
    render(&SubscribeTemplate {
        frame: Frame::load(&state.pool, user, csrf_token, &i18n).await?,
        feed_url: format!("{}/feed", origin(state, headers)),
        feed_readable: state.is_public_mode(),
        email,
        i18n,
    })
}

/// A visitor asks to subscribe. The answer is the same whatever the
/// address, past the limits included: only a mistake in the form is said.
pub async fn ask(
    State(state): State<AppState>,
    OptionalUser(user): OptionalUser,
    FormCsrfToken(csrf_token): FormCsrfToken,
    headers: HeaderMap,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Locale(i18n): Locale,
    HtmlForm(input): HtmlForm<VisitorForm>,
) -> Result<Response, AppError> {
    let base = link_base(&state).await?;
    let (Some(base), true, None) = (base, visitors_welcome(&state).await?, &user) else {
        return Ok(Redirect::to("/subscribe").into_response());
    };
    let choices = input.choices();
    let card = match checked_address(&input.email, choices) {
        Err((spot, key)) => {
            let error = Some((spot, i18n.t(key).to_string()));
            EmailCard::form(input.email.trim(), choices, error)
        }
        Ok(email) => {
            let ip = request_ip(&state, &headers, connect_info);
            if state.login_limiter.count_subscribe_request(&ip) == RequestCheck::Allowed {
                ask_later(&state, &email, base, choices, i18n.locale());
            }
            EmailCard {
                step: Step::Asked,
                notice: Some(i18n.tf("subscribe.asked", &[("email", &email)])),
                ..EmailCard::form(&email, choices, None)
            }
        }
    };
    render_page(&state, None, &headers, csrf_token, Some(card), i18n).await
}

/// The address, cleaned, or what is wrong with the form and the key of
/// its words.
fn checked_address(email: &str, choices: Choices) -> Result<String, (Spot, &'static str)> {
    let email = AuthService::normalize_email(email)
        .map_err(|_| (Spot::Address, "validation.email_invalid"))?;
    if !choices.any() {
        return Err((Spot::Boxes, "subscribe.no_choice"));
    }
    Ok(email)
}

fn ask_later(state: &AppState, email: &str, base: String, choices: Choices, locale: &'static str) {
    let pool = state.pool.clone();
    let notifier = Arc::clone(&state.notifier);
    let email = email.to_string();
    tokio::spawn(async move {
        if let Err(e) = send_confirmation(&pool, &notifier, &email, &base, choices, locale).await {
            tracing::error!("Subscription confirmation not issued: {e}");
        }
    });
}

/// Stores the request, unless the address or the page had enough of them
/// this hour, and emails its link in the language the page was read in.
async fn send_confirmation(
    pool: &DbPool,
    notifier: &Notifier,
    email: &str,
    base: &str,
    choices: Choices,
    locale: &str,
) -> Result<(), AppError> {
    let SubscribeRequest::Issued(link) =
        SubscriptionService::request(pool, email, locale, choices).await?
    else {
        return Ok(());
    };
    let i18n = I18n::new(locale);
    let url = format!("{base}/subscribe/confirm?token={}", link.token);
    let instance = crate::brand_name();
    let mail = ConfirmEmail {
        to: &link.email,
        url: &url,
        instance: &instance,
        i18n: &i18n,
    };
    if let Err(failure) = notifier.send_subscription_confirmation(&mail).await {
        tracing::warn!(failure = %failure.code(), "Subscription confirmation email not sent");
    }
    Ok(())
}

/// A member subscribes at the address of their account, changes their
/// boxes, or stops. Unticking every box stops too.
pub async fn save_mine(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Locale(i18n): Locale,
    HtmlForm(input): HtmlForm<MemberForm>,
) -> Result<Response, AppError> {
    if link_base(&state).await?.is_none() {
        return Ok(Redirect::to("/subscribe").into_response());
    }
    let choices = input.choices();
    let done = if input.action == "stop" || !choices.any() {
        if SubscriptionService::stop_member(&state.pool, user.id).await? {
            record_member(&state.pool, JournalKind::SubscriptionEnded, &user).await;
        }
        "stopped"
    } else {
        let created =
            SubscriptionService::save_member(&state.pool, user.id, i18n.locale(), choices).await?;
        if created {
            record_member(&state.pool, JournalKind::SubscriptionConfirmed, &user).await;
        }
        "saved"
    };
    Ok(Redirect::to(&format!("/subscribe?done={done}")).into_response())
}

async fn record_member(pool: &DbPool, kind: JournalKind, user: &User) {
    NewJournalEntry::new(kind)
        .on_member(user)
        .record(pool)
        .await;
}

#[derive(Template)]
#[template(path = "subscriptions/link.html")]
struct LinkTemplate {
    title: String,
    intro: String,
    /// The form of the page, when it asks for a click: where it posts and
    /// what its button says.
    action: Option<(String, String)>,
    /// Where to go next, and how the link says it.
    next: Option<(String, String)>,
    csrf_token: String,
    instance: String,
    powered_by: bool,
    i18n: I18n,
}

impl LinkTemplate {
    fn new(i18n: I18n, title: &str, intro: String) -> Self {
        let (instance, powered_by) = instance_title();
        Self {
            title: i18n.t(title).to_string(),
            intro,
            action: None,
            next: None,
            csrf_token: String::new(),
            instance,
            powered_by,
            i18n,
        }
    }

    fn asking(mut self, url: String, submit: &str) -> Self {
        self.action = Some((url, self.i18n.t(submit).to_string()));
        self
    }

    fn leading(mut self, href: &str, label: &str) -> Self {
        self.next = Some((href.to_string(), self.i18n.t(label).to_string()));
        self
    }

    /// A link page is never kept: its address carries a token.
    fn into_response(self) -> Result<Response, AppError> {
        Ok(no_store(render(&self)?))
    }
}

/// The page a confirmation link opens: what the visitor asked for, and the
/// button that confirms it. Opening it changes nothing.
pub async fn confirm_page(
    State(state): State<AppState>,
    Locale(i18n): Locale,
    Query(query): Query<TokenQuery>,
) -> Result<Response, AppError> {
    if !visitors_welcome(&state).await? {
        return closed(i18n).into_response();
    }
    let Some(request) = SubscriptionService::pending(&state.pool, &query.token).await? else {
        return dead_link(i18n).into_response();
    };
    let intro = i18n.tf(
        "subscription.confirm_intro",
        &[
            ("instance", &crate::brand_name()),
            ("email", &request.email),
            ("choices", &choice_words(request.choices, &i18n)),
        ],
    );
    let url = format!("/subscribe/confirm?token={}", query.token.trim());
    LinkTemplate::new(i18n, "subscription.confirm_title", intro)
        .asking(url, "subscription.confirm_submit")
        .into_response()
}

pub async fn confirm(
    State(state): State<AppState>,
    Locale(i18n): Locale,
    Query(query): Query<TokenQuery>,
) -> Result<Response, AppError> {
    if !visitors_welcome(&state).await? {
        return closed(i18n).into_response();
    }
    let Some(subscriber) = SubscriptionService::confirm(&state.pool, &query.token).await? else {
        return dead_link(i18n).into_response();
    };
    let email = subscriber.email.unwrap_or_default();
    NewJournalEntry::new(JournalKind::SubscriptionConfirmed)
        .about(email.clone())
        .record(&state.pool)
        .await;
    let intro = i18n.tf(
        "subscription.confirmed_intro",
        &[("instance", &crate::brand_name()), ("email", &email)],
    );
    LinkTemplate::new(i18n, "subscription.confirmed_title", intro)
        .leading("/", "subscription.open_page")
        .into_response()
}

fn closed(i18n: I18n) -> LinkTemplate {
    let intro = i18n.t("subscription.closed_intro").to_string();
    LinkTemplate::new(i18n, "subscription.closed_title", intro)
}

fn dead_link(i18n: I18n) -> LinkTemplate {
    let intro = i18n.t("subscription.dead_intro").to_string();
    LinkTemplate::new(i18n, "subscription.dead_title", intro)
        .leading("/subscribe", "subscription.subscribe_again")
}

/// "incidents, maintenances": the boxes ticked, in words.
pub(super) fn choice_words(choices: Choices, i18n: &I18n) -> String {
    [
        (choices.incidents, "notifications.receives_incidents"),
        (choices.maintenances, "notifications.receives_maintenances"),
        (choices.publications, "notifications.receives_publications"),
    ]
    .into_iter()
    .filter(|(ticked, _)| *ticked)
    .map(|(_, key)| i18n.t(key))
    .collect::<Vec<_>>()
    .join(", ")
}

/// The page an unsubscribe link opens: the address that stops receiving,
/// and the button. A link that no longer names anyone says it is done.
pub async fn unsubscribe_page(
    State(state): State<AppState>,
    Locale(i18n): Locale,
    Query(query): Query<TokenQuery>,
) -> Result<Response, AppError> {
    let Some(subscriber) = SubscriptionService::linked(&state.pool, &query.token).await? else {
        return unsubscribed(i18n).into_response();
    };
    let email = address_of(&state.pool, &subscriber).await?;
    let intro = i18n.tf(
        "subscription.unsubscribe_intro",
        &[("email", &email), ("instance", &crate::brand_name())],
    );
    let url = format!("/unsubscribe?token={}", query.token.trim());
    LinkTemplate::new(i18n, "subscription.unsubscribe_title", intro)
        .asking(url, "subscription.unsubscribe_submit")
        .into_response()
}

/// Ends the subscription. A mail client posts here on its own, its body
/// saying `List-Unsubscribe=One-Click`, which is not read: the token is
/// enough. The answer is the same for a token that names no one.
pub async fn unsubscribe(
    State(state): State<AppState>,
    Locale(i18n): Locale,
    Query(query): Query<TokenQuery>,
) -> Result<Response, AppError> {
    if let Some(subscriber) = SubscriptionService::unsubscribe(&state.pool, &query.token).await? {
        record_ended(&state.pool, &subscriber).await?;
    }
    unsubscribed(i18n).into_response()
}

fn unsubscribed(i18n: I18n) -> LinkTemplate {
    let intro = i18n.t("subscription.unsubscribed_intro").to_string();
    LinkTemplate::new(i18n, "subscription.unsubscribed_title", intro)
        .leading("/", "subscription.open_page")
}

async fn record_ended(pool: &DbPool, subscriber: &Subscriber) -> Result<(), AppError> {
    let entry = NewJournalEntry::new(JournalKind::SubscriptionEnded);
    let member = match subscriber.user_id {
        Some(id) => UserRepository::find_by_id(pool, id).await?,
        None => None,
    };
    let entry = match (&member, &subscriber.email) {
        (Some(member), _) => entry.on_member(member),
        (None, Some(email)) => entry.about(email.clone()),
        (None, None) => entry,
    };
    entry.record(pool).await;
    Ok(())
}

/// Where a subscriber's emails go: the address they confirmed, or their
/// account's.
pub(super) async fn address_of(pool: &DbPool, subscriber: &Subscriber) -> Result<String, AppError> {
    if let Some(email) = &subscriber.email {
        return Ok(email.clone());
    }
    let member = match subscriber.user_id {
        Some(id) => UserRepository::find_by_id(pool, id).await?,
        None => None,
    };
    Ok(member.map(|member| member.email).unwrap_or_default())
}
