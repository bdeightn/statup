//! The emails that carry a link to the person who asked for it: a password
//! reset, or the confirmation of a subscription.

use askama::Template;
use lettre::Message;
use lettre::message::{Mailbox, MultiPart};

use super::mail::own_message_id;
use crate::i18n::I18n;

/// What the link of an email does, which picks its words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkPurpose {
    PasswordReset,
    Subscription,
}

/// The keys of an email's words in the locale files.
struct Keys {
    subject: &'static str,
    text: &'static str,
    ignore: &'static str,
    action: &'static str,
}

impl LinkPurpose {
    fn keys(self) -> Keys {
        match self {
            Self::PasswordReset => Keys {
                subject: "reset_mail.subject",
                text: "reset_mail.text",
                ignore: "reset_mail.ignore",
                action: "reset_mail.action",
            },
            Self::Subscription => Keys {
                subject: "subscription_mail.subject",
                text: "subscription_mail.text",
                ignore: "subscription_mail.ignore",
                action: "subscription_mail.action",
            },
        }
    }
}

/// An email that carries a link, in its reader's language.
pub struct LinkEmail<'a> {
    pub purpose: LinkPurpose,
    pub to: &'a str,
    /// The full address of the page the link opens, token included.
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
    fn of(mail: &LinkEmail<'_>) -> Self {
        let i18n = mail.i18n;
        let keys = mail.purpose.keys();
        let names = [("instance", mail.instance), ("email", mail.to)];
        Self {
            subject: i18n.tf(keys.subject, &names),
            text: i18n.tf(keys.text, &names),
            ignore: i18n.t(keys.ignore).to_string(),
            action: i18n.t(keys.action).to_string(),
        }
    }
}

#[derive(Template)]
#[template(path = "emails/link.html")]
struct LinkHtml<'a> {
    lang: &'a str,
    words: &'a Words,
    url: &'a str,
    instance: &'a str,
}

/// The email, or `None` when the recipient's address cannot be written in
/// a message. It carries no way out: it answers a request, it is no news.
pub fn link_email(from: &Mailbox, mail: &LinkEmail<'_>) -> Option<Message> {
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

fn plain_text(words: &Words, mail: &LinkEmail<'_>) -> String {
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
fn html(words: &Words, mail: &LinkEmail<'_>) -> String {
    LinkHtml {
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

    const RESET_URL: &str = "https://status.example/password/reset?token=7.abc";
    const CONFIRM_URL: &str = "https://status.example/subscribe/confirm?token=7.abc";

    fn mail<'a>(
        purpose: LinkPurpose,
        to: &'a str,
        instance: &'a str,
        i18n: &'a I18n,
    ) -> LinkEmail<'a> {
        let url = match purpose {
            LinkPurpose::PasswordReset => RESET_URL,
            LinkPurpose::Subscription => CONFIRM_URL,
        };
        LinkEmail {
            purpose,
            to,
            url,
            instance,
            i18n,
        }
    }

    fn written(purpose: LinkPurpose, to: &str, locale: &str) -> Option<String> {
        let i18n = I18n::new(locale);
        let from: Mailbox = "Statup <status@example.com>".parse().unwrap();
        link_email(&from, &mail(purpose, to, "ACME <IT>", &i18n))
            .map(|message| String::from_utf8(message.formatted()).unwrap())
    }

    #[test]
    fn the_email_carries_the_link_in_both_versions() {
        let i18n = I18n::new("en");
        for purpose in [LinkPurpose::PasswordReset, LinkPurpose::Subscription] {
            let mail = mail(purpose, "ana@example.com", "ACME <IT>", &i18n);
            let words = Words::of(&mail);

            assert!(plain_text(&words, &mail).contains(mail.url));
            let page = html(&words, &mail);
            assert!(page.contains(&format!("href=\"{}\"", mail.url)));
            assert!(page.contains("lang=\"en\""));
            assert!(!page.contains("ACME <IT>"), "the instance name is escaped");
        }
    }

    #[test]
    fn the_email_goes_to_its_reader_from_the_sender_without_a_way_out() {
        for purpose in [LinkPurpose::PasswordReset, LinkPurpose::Subscription] {
            let text = written(purpose, "ana@example.com", "fr").unwrap();

            assert!(text.contains("To: ana@example.com"));
            assert!(text.contains("From: Statup <status@example.com>"));
            assert!(text.contains("Message-ID: <statup."));
            assert!(!text.contains("List-Unsubscribe"));
        }
    }

    #[test]
    fn each_purpose_has_its_own_words_in_the_readers_language() {
        let en = I18n::new("en");
        let fr = I18n::new("fr");
        let subject =
            |purpose, i18n| Words::of(&mail(purpose, "ana@example.com", "ACME", i18n)).subject;

        assert_ne!(
            subject(LinkPurpose::PasswordReset, &en),
            subject(LinkPurpose::PasswordReset, &fr)
        );
        assert_ne!(
            subject(LinkPurpose::PasswordReset, &en),
            subject(LinkPurpose::Subscription, &en)
        );
        assert_eq!(
            subject(LinkPurpose::Subscription, &en),
            "ACME: confirm your subscription"
        );
        let text = Words::of(&mail(
            LinkPurpose::Subscription,
            "ana@example.com",
            "ACME",
            &en,
        ))
        .text;
        assert!(text.contains("ACME") && text.contains("ana@example.com"));
    }

    #[test]
    fn an_address_that_cannot_be_written_sends_nothing() {
        assert!(written(LinkPurpose::PasswordReset, "not an address", "en").is_none());
    }
}
