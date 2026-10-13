//! The email that tells a member their announced incident update is overdue.

use askama::Template;
use lettre::Message;
use lettre::message::{Mailbox, MultiPart};

use super::mail::own_message_id;
use crate::i18n::I18n;

/// What the reminder says, in the member's language.
pub struct ReminderEmail<'a> {
    pub to: &'a str,
    /// The incident's title, as its author wrote it.
    pub title: &'a str,
    /// When the update was announced for, as the instance clock shows it
    /// ("à 14:30", "demain à 09:00").
    pub due: &'a str,
    /// How long it has been overdue ("5 min").
    pub late: &'a str,
    /// The incident's page, when the instance knows its public address.
    pub url: Option<&'a str>,
    pub instance: &'a str,
    pub i18n: &'a I18n,
}

/// The words of the email, shared by its plain text and HTML versions.
struct Words {
    subject: String,
    text: String,
    why: String,
    action: String,
}

impl Words {
    fn of(mail: &ReminderEmail<'_>) -> Self {
        let i18n = mail.i18n;
        Self {
            subject: i18n.tf("reminder_mail.subject", &[("title", mail.title)]),
            text: i18n.tf(
                "reminder_mail.text",
                &[
                    ("title", mail.title),
                    ("due", mail.due),
                    ("late", mail.late),
                ],
            ),
            why: i18n.t("reminder_mail.why").to_string(),
            action: i18n.t("reminder_mail.action").to_string(),
        }
    }
}

#[derive(Template)]
#[template(path = "emails/update_reminder.html")]
struct ReminderHtml<'a> {
    lang: &'a str,
    words: &'a Words,
    url: Option<&'a str>,
    instance: &'a str,
}

/// The email, or `None` when the recipient's address cannot be written in
/// a message.
pub fn reminder_email(from: &Mailbox, mail: &ReminderEmail<'_>) -> Option<Message> {
    let to: Mailbox = mail.to.parse().ok()?;
    let words = Words::of(mail);
    Message::builder()
        .from(from.clone())
        .to(to)
        .subject(words.subject.clone())
        .message_id(Some(own_message_id(from)))
        .multipart(MultiPart::alternative_plain_html(
            plain_text(&words, mail),
            html(&words, mail),
        ))
        .ok()
}

fn plain_text(words: &Words, mail: &ReminderEmail<'_>) -> String {
    let mut lines = vec![words.text.as_str(), "", words.why.as_str(), ""];
    if let Some(url) = mail.url {
        lines.extend([words.action.as_str(), url, ""]);
    }
    lines.push(mail.instance);
    lines.join("\n")
}

/// The plain text alone is sent if the HTML version ever fails to render.
fn html(words: &Words, mail: &ReminderEmail<'_>) -> String {
    ReminderHtml {
        lang: mail.i18n.locale(),
        words,
        url: mail.url,
        instance: mail.instance,
    }
    .render()
    .unwrap_or_else(|_| plain_text(words, mail))
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://status.example/events/7";

    fn mail_for<'a>(
        to: &'a str,
        title: &'a str,
        url: Option<&'a str>,
        i18n: &'a I18n,
    ) -> ReminderEmail<'a> {
        ReminderEmail {
            to,
            title,
            due: "at 14:30",
            late: "5 min",
            url,
            instance: "ACME <IT>",
            i18n,
        }
    }

    fn written(to: &str, locale: &str) -> Option<String> {
        let i18n = I18n::new(locale);
        let mail = mail_for(to, "Mail outage", Some(URL), &i18n);
        let from: Mailbox = "Statup <status@example.com>".parse().unwrap();
        reminder_email(&from, &mail).map(|message| String::from_utf8(message.formatted()).unwrap())
    }

    #[test]
    fn the_email_goes_to_the_member_from_the_sender() {
        let text = written("ana@example.com", "fr").unwrap();

        assert!(text.contains("To: ana@example.com"));
        assert!(text.contains("From: Statup <status@example.com>"));
        assert!(text.contains("Message-ID: <statup."));
    }

    #[test]
    fn the_subject_carries_the_title_in_the_members_language() {
        let en = I18n::new("en");
        let fr = I18n::new("fr");
        let subject =
            |i18n: &I18n| Words::of(&mail_for("a@b.co", "Mail outage", None, i18n)).subject;

        assert!(subject(&en).contains("Mail outage"));
        assert!(subject(&fr).contains("Mail outage"));
        assert_ne!(subject(&en), subject(&fr));
    }

    #[test]
    fn the_text_says_when_it_was_due_and_how_late() {
        let i18n = I18n::new("en");
        let mail = mail_for("a@b.co", "Mail outage", None, &i18n);
        let text = plain_text(&Words::of(&mail), &mail);

        assert!(text.contains("Mail outage"));
        assert!(text.contains("at 14:30"));
        assert!(text.contains("5 min"));
    }

    #[test]
    fn with_an_address_both_versions_carry_the_link() {
        let i18n = I18n::new("en");
        let mail = mail_for("a@b.co", "Mail outage", Some(URL), &i18n);
        let words = Words::of(&mail);

        assert!(plain_text(&words, &mail).contains(URL));
        assert!(plain_text(&words, &mail).contains(&words.action));
        let page = html(&words, &mail);
        assert!(page.contains(&format!("href=\"{URL}\"")));
        assert!(page.contains("lang=\"en\""));
    }

    #[test]
    fn without_an_address_there_is_no_action() {
        let i18n = I18n::new("en");
        let mail = mail_for("a@b.co", "Mail outage", None, &i18n);
        let words = Words::of(&mail);
        let page = html(&words, &mail);

        assert!(!plain_text(&words, &mail).contains(&words.action));
        assert!(!page.contains(&words.action));
        assert!(!page.contains("href="));
    }

    #[test]
    fn the_title_and_the_instance_are_escaped_in_the_html() {
        let i18n = I18n::new("en");
        let mail = mail_for("a@b.co", "Mail <down>", Some(URL), &i18n);
        let page = html(&Words::of(&mail), &mail);

        assert!(!page.contains("Mail <down>"));
        assert!(!page.contains("ACME <IT>"));
        assert!(page.contains("Mail &#60;down&#62;"));
    }

    #[test]
    fn an_address_that_cannot_be_written_sends_nothing() {
        assert!(written("not an address", "en").is_none());
    }
}
