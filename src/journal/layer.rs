//! Catches the errors and warnings the app logs, so that they reach the
//! Journal page without each call site knowing about it.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{FilterFn, Filtered, LevelFilter};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

use super::{TARGET, scrub};
use crate::models::JournalKind;
use crate::repositories::NewJournalEntry;

/// Lines waiting for the writer; past this, new ones are dropped rather
/// than slowing the request that logged them.
const QUEUE: usize = 1024;

/// The name of the span opened for each request, whose method and path
/// say where an error happened.
const REQUEST_SPAN: &str = "request";

/// What the writer reads: the lines caught, and how many were dropped.
pub struct JournalInbox {
    pub receiver: mpsc::Receiver<NewJournalEntry>,
    pub dropped: Arc<AtomicU64>,
}

pub struct JournalLayer {
    sender: mpsc::Sender<NewJournalEntry>,
    dropped: Arc<AtomicU64>,
}

/// The layer, filtered to the request spans and to the warnings and
/// errors, whatever level the server output is set to.
pub fn journal_layer<S>() -> (FilteredJournalLayer<S>, JournalInbox)
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let (sender, receiver) = mpsc::channel(QUEUE);
    let dropped = Arc::new(AtomicU64::new(0));
    let layer = JournalLayer {
        sender,
        dropped: Arc::clone(&dropped),
    };
    let wanted: fn(&Metadata<'_>) -> bool = |meta| {
        if meta.is_span() {
            meta.name() == REQUEST_SPAN
        } else {
            *meta.level() <= Level::WARN
        }
    };
    (
        // The request spans are INFO: the hint keeps every DEBUG and TRACE
        // callsite skipped up front when the output does not want them.
        layer.with_filter(FilterFn::new(wanted).with_max_level_hint(LevelFilter::INFO)),
        JournalInbox { receiver, dropped },
    )
}

/// The journal layer with its own filter.
pub type FilteredJournalLayer<S> = Filtered<JournalLayer, FilterFn<fn(&Metadata<'_>) -> bool>, S>;

/// The method and path of a request, kept on its span.
struct RequestLine(String);

impl<S> Layer<S> for JournalLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if attrs.metadata().name() != REQUEST_SPAN {
            return;
        }
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        if let Some(span) = ctx.span(id) {
            let line: Vec<&str> = fields
                .pairs
                .iter()
                .map(|(_, value)| value.as_str())
                .collect();
            span.extensions_mut().insert(RequestLine(line.join(" ")));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if !is_caught(event.metadata()) {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let request = ctx.event_scope(event).and_then(|scope| {
            scope.from_root().find_map(|span| {
                span.extensions()
                    .get::<RequestLine>()
                    .map(|line| line.0.clone())
            })
        });
        if self
            .sender
            .try_send(caught(event, fields, request))
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// The app's own warnings and errors, except the lines [`super::record`]
/// already keeps.
fn is_caught(meta: &Metadata<'_>) -> bool {
    let target = meta.target();
    let ours = target == "statup" || target.starts_with("statup::");
    ours && !target.starts_with(TARGET) && *meta.level() <= Level::WARN
}

fn caught(event: &Event<'_>, fields: Fields, request: Option<String>) -> NewJournalEntry {
    let kind = if *event.metadata().level() == Level::ERROR {
        JournalKind::AppError
    } else {
        JournalKind::AppWarning
    };
    let named = fields
        .pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"));
    let detail: Vec<String> = request.into_iter().chain(named).collect();
    let mut entry = NewJournalEntry::new(kind).about(scrub(&fields.message));
    if !detail.is_empty() {
        entry = entry.with_detail(scrub(&detail.join(" · ")));
    }
    entry
}

/// The message of an event, and its other fields as `name=value`.
#[derive(Default)]
struct Fields {
    message: String,
    pairs: Vec<(&'static str, String)>,
}

impl Fields {
    fn push(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message = value;
        } else {
            self.pairs.push((field.name(), value));
        }
    }
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.push(field, value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.push(field, format!("{value:?}"));
    }
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::Registry;
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    fn catch(emit: impl FnOnce()) -> Vec<NewJournalEntry> {
        let (layer, mut inbox) = journal_layer::<Registry>();
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, emit);
        let mut caught = Vec::new();
        while let Ok(entry) = inbox.receiver.try_recv() {
            caught.push(entry);
        }
        caught
    }

    #[test]
    fn an_error_in_a_request_says_where_it_happened() {
        let caught = catch(|| {
            let span = tracing::info_span!("request", method = %"POST", path = %"/admin/settings");
            let _inside = span.enter();
            tracing::error!(target: "statup::error", code = 7, "Database error: locked");
        });

        assert_eq!(caught.len(), 1);
        assert_eq!(caught[0].kind, JournalKind::AppError);
        assert_eq!(caught[0].subject.as_deref(), Some("Database error: locked"));
        assert_eq!(
            caught[0].detail.as_deref(),
            Some("POST /admin/settings · code=7")
        );
    }

    #[test]
    fn only_the_apps_own_warnings_and_errors_are_caught() {
        let caught = catch(|| {
            tracing::info!(target: "statup::routes", "Settings saved");
            tracing::warn!(target: "hyper::proto", "Connection reset");
            tracing::warn!(target: TARGET, "Journal line not saved");
            tracing::warn!(target: "statup::services", "Round skipped");
        });

        assert_eq!(caught.len(), 1);
        assert_eq!(caught[0].kind, JournalKind::AppWarning);
        assert_eq!(caught[0].subject.as_deref(), Some("Round skipped"));
        assert_eq!(caught[0].detail, None);
    }

    #[test]
    fn a_secret_address_in_an_error_is_scrubbed() {
        let caught = catch(|| {
            tracing::error!(
                target: "statup::error",
                "Internal error: url (https://hooks.slack.com/services/T/B/SECRET)"
            );
        });

        assert_eq!(
            caught[0].subject.as_deref(),
            Some("Internal error: url (https://hooks.slack.com/…)")
        );
    }
}
