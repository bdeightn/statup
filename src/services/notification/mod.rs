//! Notifications: what Statup tells the destinations an admin set up.

mod deliver;
mod destinations;
mod format;
mod mail;
mod notice;
mod queue;
mod reminder_mail;
mod reminders;
mod reset_mail;
mod sender;
mod subscribe_mail;
mod subscribers;

pub(crate) use deliver::{PAGE_ADDRESS_SETTING, channel_detail};
pub use deliver::{deliver_due, page_address, spawn_notifications};
pub use destinations::{destination_allowed, destination_name_refusal, email_addresses};
pub use format::*;
pub use mail::Unsubscribe;
pub use notice::*;
pub use queue::{notify, notify_later};
pub use reminder_mail::ReminderEmail;
pub use reminders::remind_overdue;
pub use reset_mail::ResetEmail;
pub use sender::{Failure, Notifier, SEND_TIMEOUT, Sender, SubscriberEmail, SubscriberSender};
pub use subscribe_mail::ConfirmEmail;
pub use subscribers::{Round, deliver_to_subscribers, given_up_key, spawn_subscriber_mail};
