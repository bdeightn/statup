//! The email that asks a visitor to confirm their subscription.

use askama::Template;
use lettre::Message;
use lettre::message::{Mailbox, MultiPart};

use super::mail::own_message_id;
use crate::i18n::I18n;

/// What the confirmation email says, in the visitor's language.
pub struct ConfirmEmail<'a> {
    pub to: &'a str,
    /// The full address of the confirmation page, token included.
    pub url: &'a str,
    pub instance: &'a str,
    pub i18n: &'a I18n,
}

/// The words of the email, shared by its plain text and HTML versions.
struct Words {
    subject: String,
    text: String,
    ignore: String,
    action: String,
}

impl Words {
    fn of(mail: &ConfirmEmail<'_>) -> Self {
        let i18n = mail.i18n;
        Self {
            subject: i18n.tf("subscription_mail.subject", &[("instance", mail.instance)]),
            text: i18n.tf(
                "subscription_mail.text",
                &[("instance", mail.instance), ("email", mail.to)],
            ),
            ignore: i18n.t("subscription_mail.ignore").to_string(),
            action: i18n.t("subscription_mail.action").to_string(),
        }
    }
}

#[derive(Template)]
#[template(path = "emails/subscription_confirm.html")]
struct ConfirmHtml<'a> {
    lang: &'a str,
    words: &'a Words,
    url: &'a str,
    instance: &'a str,
}

/// The email, or `None` when the recipient's address cannot be written in
/// a message. It carries no way out: nothing is subscribed yet.
pub fn confirm_email(from: &Mailbox, mail: &ConfirmEmail<'_>) -> Option<Message> {
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

fn plain_text(words: &Words, mail: &ConfirmEmail<'_>) -> String {
    [
        words.text.as_str(),
        "",
        words.action.as_str(),
        mail.url,
        "",
        words.ignore.as_str(),
        "",
        mail.instance,
    ]
    .join("\n")
}

/// The plain text alone is sent if the HTML version ever fails to render.
fn html(words: &Words, mail: &ConfirmEmail<'_>) -> String {
    ConfirmHtml {
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

    const URL: &str = "https://status.example/subscribe/confirm?token=7.abc";

    fn written(to: &str, locale: &str) -> Option<String> {
        let i18n = I18n::new(locale);
        let mail = ConfirmEmail {
            to,
            url: URL,
            instance: "ACME <IT>",
            i18n: &i18n,
        };
        let from: Mailbox = "Statup <status@example.com>".parse().unwrap();
        confirm_email(&from, &mail).map(|message| String::from_utf8(message.formatted()).unwrap())
    }

    #[test]
    fn the_email_carries_the_link_in_both_versions() {
        let i18n = I18n::new("en");
        let mail = ConfirmEmail {
            to: "ana@example.com",
            url: URL,
            instance: "ACME <IT>",
            i18n: &i18n,
        };
        let words = Words::of(&mail);

        assert!(plain_text(&words, &mail).contains(URL));
        let page = html(&words, &mail);
        assert!(page.contains(&format!("href=\"{URL}\"")));
        assert!(page.contains("lang=\"en\""));
        assert!(!page.contains("ACME <IT>"), "the instance name is escaped");
    }

    #[test]
    fn the_subject_and_text_name_the_instance_and_the_address() {
        let i18n = I18n::new("en");
        let mail = ConfirmEmail {
            to: "ana@example.com",
            url: URL,
            instance: "ACME",
            i18n: &i18n,
        };
        let words = Words::of(&mail);

        assert_eq!(words.subject, "ACME: confirm your subscription");
        assert!(words.text.contains("ACME"));
        assert!(words.text.contains("ana@example.com"));
    }

    #[test]
    fn the_email_goes_to_the_visitor_without_a_way_out() {
        let text = written("ana@example.com", "fr").unwrap();

        assert!(text.contains("To: ana@example.com"));
        assert!(text.contains("From: Statup <status@example.com>"));
        assert!(text.contains("Message-ID: <statup."));
        assert!(!text.contains("List-Unsubscribe"));
    }

    #[test]
    fn an_address_that_cannot_be_written_sends_nothing() {
        assert!(written("not an address", "en").is_none());
    }
}
