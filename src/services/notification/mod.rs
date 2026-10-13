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

pub(crate) use deliver::PAGE_ADDRESS_SETTING;
pub use deliver::{deliver_due, page_address, spawn_notifications};
pub use destinations::{destination_allowed, destination_name_refusal, email_addresses};
pub use format::*;
pub use notice::*;
pub use queue::notify;
pub use reminder_mail::ReminderEmail;
pub use reminders::remind_overdue;
pub use reset_mail::ResetEmail;
pub use sender::{Failure, Notifier, SEND_TIMEOUT, Sender};
