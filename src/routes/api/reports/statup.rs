//! The format of Statup itself, for the tools whose webhook body can be
//! written and for scripts: `service` (a name or a slug), `state` (`down`
//! or `up`) and an optional `message`, as JSON or as a form.

use axum::http::StatusCode;
use axum::response::Response;
use serde_json::{Map, Value};

use super::{Reported, Signal};
use crate::routes::api::error::refusal;

/// What makes a body in this format unusable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unusable {
    NoService,
    NoState,
}

impl Unusable {
    pub(super) fn answer(self) -> Response {
        match self {
            Self::NoService => refusal(
                StatusCode::BAD_REQUEST,
                "service_required",
                "Name the service, by its name or its slug.",
            ),
            Self::NoState => refusal(
                StatusCode::BAD_REQUEST,
                "invalid_state",
                "The state is down or up.",
            ),
        }
    }
}

/// `None` when the body is not in this format.
pub(super) fn read(body: &Value) -> Option<Result<Signal, Unusable>> {
    let fields = body.as_object()?;
    if !fields.contains_key("service") && !fields.contains_key("state") {
        return None;
    }
    Some(signal(fields))
}

fn signal(fields: &Map<String, Value>) -> Result<Signal, Unusable> {
    let text = |name: &str| {
        fields
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let service = text("service").ok_or(Unusable::NoService)?;
    let state = match text("state").map(str::to_lowercase).as_deref() {
        Some("down") => Reported::Down,
        Some("up") => Reported::Up,
        _ => return Err(Unusable::NoState),
    };
    Ok(Signal {
        service: service.to_string(),
        state,
        message: text("message").map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_service_and_a_state_make_a_signal() {
        let signal = read(&json!({"service": " Mail ", "state": "DOWN", "message": "No SMTP"}));
        assert_eq!(
            signal.map(Result::ok),
            Some(Some(Signal {
                service: "Mail".to_string(),
                state: Reported::Down,
                message: Some("No SMTP".to_string()),
            }))
        );
    }

    #[test]
    fn a_missing_service_or_a_wrong_state_is_refused() {
        let unusable = |body| read(&body).and_then(Result::err);
        assert_eq!(
            unusable(json!({"state": "down"})),
            Some(Unusable::NoService)
        );
        assert_eq!(
            unusable(json!({"service": "Mail", "state": "broken"})),
            Some(Unusable::NoState)
        );
        assert_eq!(
            unusable(json!({"service": "Mail"})),
            Some(Unusable::NoState)
        );
        assert_eq!(Unusable::NoState.answer().status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn another_body_is_not_in_this_format() {
        assert!(read(&json!({"alerts": []})).is_none());
        assert!(read(&json!(["service"])).is_none());
    }
}
