//! A notice as an email: a subject, a plain text body with a short HTML one,
//! and the headers that keep the messages of one event in one conversation.

use askama::Template;
use lettre::Message;
use lettre::message::header::{HeaderName, HeaderValue};
use lettre::message::{Mailbox, MessageBuilder, MultiPart};
use uuid::Uuid;

use super::format::marked;
use super::{Facts, Notice};
use crate::i18n::I18n;
use crate::models::Happening;

/// The recipients of an email destination, as stored: addresses separated
/// by commas.
pub fn recipients(target: &str) -> Option<Vec<Mailbox>> {
    target
        .split(',')
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .map(|address| address.parse().ok())
        .collect()
}

/// The way out that every email to a subscriber carries.
pub struct Unsubscribe {
    /// The unsubscribe page, token included.
    pub url: String,
    /// Why they receive this email, in their language.
    pub reason: String,
    pub label: String,
}

impl Unsubscribe {
    /// `base` is the page address without a trailing slash, `token` the
    /// subscriber's unsubscribe token.
    pub fn new(base: &str, token: &str, instance: &str, i18n: &I18n) -> Self {
        Self {
            url: format!("{base}/unsubscribe?token={token}"),
            reason: i18n.tf("subscriber_mail.reason", &[("instance", instance)]),
            label: i18n.t("subscriber_mail.unsubscribe").to_string(),
        }
    }
}

/// The email of a notice to one address: each recipient gets a copy of
/// their own, so none sees the others and a refused address stops no one.
/// A subscriber's copy also carries the way out.
pub fn email(
    from: &Mailbox,
    to: &Mailbox,
    notice: &Notice,
    facts: &Facts,
    locale: &str,
    unsubscribe: Option<&Unsubscribe>,
) -> Result<Message, lettre::error::Error> {
    // One subject for every message of an event, without the state's
    // square: some clients split a conversation whose subject changes.
    let mut builder = Message::builder()
        .from(from.clone())
        .to(to.clone())
        .subject(notice.headline.clone());
    let own_id = own_message_id(from);
    builder = match facts.event.as_ref().map(|event| thread_id(event.id, from)) {
        Some(thread) if starts_a_thread(facts.happening) => builder.message_id(Some(thread)),
        Some(thread) => builder
            .message_id(Some(own_id))
            .in_reply_to(thread.clone())
            .references(thread),
        None => builder.message_id(Some(own_id)),
    };
    builder = with_way_out(builder, unsubscribe);
    builder.multipart(MultiPart::alternative_plain_html(
        plain_text(notice, unsubscribe),
        html(notice, locale, unsubscribe),
    ))
}

/// The headers that let a mail client offer "unsubscribe" by itself.
fn with_way_out(builder: MessageBuilder, unsubscribe: Option<&Unsubscribe>) -> MessageBuilder {
    let Some(way_out) = unsubscribe else {
        return builder;
    };
    let builder = builder.raw_header(HeaderValue::new(
        HeaderName::new_from_ascii_str("List-Unsubscribe"),
        format!("<{}>", way_out.url),
    ));
    // The one-click form (RFC 8058) is only valid over HTTPS: mail providers
    // ignore it, or distrust the message, otherwise.
    if !way_out.url.starts_with("https://") {
        return builder;
    }
    builder.raw_header(HeaderValue::new(
        HeaderName::new_from_ascii_str("List-Unsubscribe-Post"),
        "List-Unsubscribe=One-Click".to_string(),
    ))
}

/// An id of its own, at the sender's domain: spam filters distrust a
/// message without one, and the server's name stays out of the headers.
pub(super) fn own_message_id(from: &Mailbox) -> String {
    format!(
        "<statup.{}@{}>",
        Uuid::new_v4().simple(),
        from.email.domain()
    )
}

/// The first message of an event names the conversation the next ones join.
fn starts_a_thread(happening: &str) -> bool {
    [Happening::Opened, Happening::Published]
        .iter()
        .any(|first| first.as_str() == happening)
}

fn thread_id(event_id: i64, from: &Mailbox) -> String {
    format!("<statup.event.{event_id}@{}>", from.email.domain())
}

fn plain_text(notice: &Notice, unsubscribe: Option<&Unsubscribe>) -> String {
    let mut lines = vec![marked(notice, &notice.headline)];
    lines.extend(notice.summary.clone());
    if let Some(text) = &notice.text {
        lines.extend([String::new(), text.clone()]);
    }
    if let Some(link) = &notice.link {
        lines.extend([String::new(), link.label.clone(), link.url.clone()]);
    }
    lines.extend([String::new(), notice.instance.clone()]);
    if let Some(way_out) = unsubscribe {
        lines.extend([
            String::new(),
            way_out.reason.clone(),
            way_out.label.clone(),
            way_out.url.clone(),
        ]);
    }
    lines.join("\n")
}

#[derive(Template)]
#[template(path = "emails/notice.html")]
struct NoticeEmail<'a> {
    lang: &'a str,
    headline: String,
    notice: &'a Notice,
    unsubscribe: Option<&'a Unsubscribe>,
}

/// A short HTML version, for the clients that show it. Writing it cannot
/// fail with these fields; the plain text alone is sent if it ever does.
fn html(notice: &Notice, locale: &str, unsubscribe: Option<&Unsubscribe>) -> String {
    NoticeEmail {
        lang: locale,
        headline: marked(notice, &notice.headline),
        notice,
        unsubscribe,
    }
    .render()
    .unwrap_or_else(|_| plain_text(notice, unsubscribe))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::notification::{Link, Mark};

    fn notice() -> Notice {
        Notice {
            mark: Mark::Outage,
            headline: "Incident majeur\u{a0}· Coupure <réseau>".to_string(),
            summary: Some("En analyse\u{a0}· Réseau".to_string()),
            text: Some("Le prestataire intervient.".to_string()),
            link: Some(Link {
                url: "https://status.example/events/7".to_string(),
                label: "Voir le détail".to_string(),
            }),
            instance: "ACME".to_string(),
        }
    }

    fn facts(happening: &'static str, event_id: Option<i64>) -> Facts {
        Facts {
            version: 1,
            happening,
            occurred_at: "2026-10-07T09:00:00Z".to_string(),
            instance: "ACME".to_string(),
            event: event_id.map(|id| super::super::EventFacts {
                id,
                kind: "incident",
                severity: Some("critical"),
                category: None,
                state: Some("investigating"),
                title: "Coupure".to_string(),
                services: Vec::new(),
                planned_start: None,
                planned_end: None,
                url: None,
            }),
            service: None,
            services: Vec::new(),
        }
    }

    fn sender() -> Mailbox {
        "Statup <status@example.com>".parse().unwrap()
    }

    fn written(happening: &'static str, event_id: Option<i64>) -> (Message, String) {
        let to = recipients("Board@Example.com").unwrap();
        let message = email(
            &sender(),
            &to[0],
            &notice(),
            &facts(happening, event_id),
            "fr",
            None,
        )
        .unwrap();
        let text = String::from_utf8(message.formatted()).unwrap();
        (message, text)
    }

    #[test]
    fn the_recipients_are_read_from_the_stored_list() {
        let parsed = recipients(" it@example.com ,board@example.com, ").unwrap();
        assert_eq!(parsed.len(), 2);
        assert!(recipients("it@example.com, not an address").is_none());
    }

    #[test]
    fn each_address_gets_a_copy_of_its_own() {
        let (message, text) = written("opened", Some(7));

        let envelope: Vec<String> = message
            .envelope()
            .to()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(envelope, ["Board@Example.com"]);
        assert!(text.contains("To: Board@Example.com"));
        assert!(text.contains("From: Statup <status@example.com>"));
    }

    #[test]
    fn the_first_message_of_an_event_starts_its_conversation() {
        let (_, opened) = written("opened", Some(7));
        let (_, update) = written("updated", Some(7));
        let (_, test) = written("test", None);

        assert!(opened.contains("Message-ID: <statup.event.7@example.com>"));
        assert!(update.contains("In-Reply-To: <statup.event.7@example.com>"));
        assert!(update.contains("References: <statup.event.7@example.com>"));
        assert!(!test.contains("statup.event"));
        for email in [&update, &test] {
            assert!(
                email.contains("Message-ID: <statup.") && email.contains("@example.com>"),
                "an id of its own at the sender's domain: {email}"
            );
        }
    }

    #[test]
    fn the_subject_keeps_the_headline_without_the_square() {
        let (message, _) = written("closed", Some(7));
        let subject = message.headers().get::<lettre::message::header::Subject>();
        assert_eq!(
            subject.as_ref().map(AsRef::as_ref),
            Some("Incident majeur\u{a0}· Coupure <réseau>")
        );
    }

    #[test]
    fn the_body_reads_as_plain_text_and_as_escaped_html() {
        let plain = plain_text(&notice(), None);
        assert_eq!(
            plain,
            "🟥 Incident majeur\u{a0}· Coupure <réseau>\nEn analyse\u{a0}· Réseau\n\n\
             Le prestataire intervient.\n\nVoir le détail\nhttps://status.example/events/7\n\nACME"
        );
        let page = html(&notice(), "fr", None);
        assert!(page.contains("lang=\"fr\""));
        assert!(
            page.contains("Coupure &#60;réseau&#62;") || page.contains("Coupure &lt;réseau&gt;")
        );
        assert!(page.contains("href=\"https://status.example/events/7\""));
    }

    fn way_out(base: &str) -> Unsubscribe {
        Unsubscribe::new(base, "7.abc", "ACME", &I18n::new("en"))
    }

    fn written_for_subscriber(way_out: Option<&Unsubscribe>) -> String {
        let to = recipients("ana@example.com").unwrap();
        let message = email(
            &sender(),
            &to[0],
            &notice(),
            &facts("opened", Some(7)),
            "en",
            way_out,
        )
        .unwrap();
        String::from_utf8(message.formatted()).unwrap()
    }

    fn count(text: &str, line: &str) -> usize {
        text.lines().filter(|found| *found == line).count()
    }

    #[test]
    fn the_way_out_is_built_from_the_page_and_the_token() {
        let en = way_out("https://status.example");

        assert_eq!(en.url, "https://status.example/unsubscribe?token=7.abc");
        assert_eq!(
            en.reason,
            "You receive this message because you follow the news of ACME."
        );
        assert_eq!(en.label, "Unsubscribe");
        let fr = Unsubscribe::new("https://status.example", "7.abc", "ACME", &I18n::new("fr"));
        assert_eq!(fr.label, "Se désabonner");
    }

    #[test]
    fn a_secure_page_gets_both_unsubscribe_headers() {
        let text = written_for_subscriber(Some(&way_out("https://status.example")));

        assert_eq!(
            count(
                &text,
                "List-Unsubscribe: <https://status.example/unsubscribe?token=7.abc>"
            ),
            1
        );
        assert_eq!(
            count(&text, "List-Unsubscribe-Post: List-Unsubscribe=One-Click"),
            1
        );
    }

    #[test]
    fn a_page_without_https_gets_no_one_click_header() {
        let text = written_for_subscriber(Some(&way_out("http://status.example")));

        assert_eq!(
            count(
                &text,
                "List-Unsubscribe: <http://status.example/unsubscribe?token=7.abc>"
            ),
            1
        );
        assert!(!text.contains("List-Unsubscribe-Post"));
    }

    #[test]
    fn a_channel_email_carries_no_unsubscribe_header() {
        let text = written_for_subscriber(None);

        assert!(!text.contains("List-Unsubscribe"));
    }

    #[test]
    fn both_versions_end_with_the_way_out() {
        let way_out = way_out("https://status.example");
        let url = &way_out.url;

        let plain = plain_text(&notice(), Some(&way_out));
        assert!(plain.ends_with(&format!("\n\n{}\nUnsubscribe\n{url}", way_out.reason)));
        let page = html(&notice(), "en", Some(&way_out));
        assert!(page.contains(&way_out.reason));
        assert!(page.contains(&format!("<a href=\"{url}\">Unsubscribe</a>")));
        assert!(!html(&notice(), "en", None).contains("way-out\""));
    }
}
