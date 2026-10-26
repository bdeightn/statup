//! The default webhook of Uptime Kuma: a `heartbeat` with its `status` (0
//! down, 1 up, 2 pending, 3 maintenance) and the `monitor` with its
//! `name`. Its Test button sends both as null.

use serde_json::Value;

use super::{Ignored, Reading, Reported, Signal};

/// `None` when the body is not in this format.
pub(super) fn read(body: &Value) -> Option<Reading> {
    let fields = body.as_object()?;
    let heartbeat = fields.get("heartbeat")?;
    if !fields.contains_key("monitor") {
        return None;
    }
    let mut reading = Reading::default();
    if heartbeat.is_null() {
        return Some(reading);
    }
    let Some(service) = body.pointer("/monitor/name").and_then(Value::as_str) else {
        reading.ignored.push(Ignored {
            service: None,
            reason: "no_monitor_name",
        });
        return Some(reading);
    };
    let state = match heartbeat.get("status").and_then(Value::as_i64) {
        Some(0) => Reported::Down,
        Some(1) => Reported::Up,
        _ => {
            reading.ignored.push(Ignored {
                service: Some(service.to_string()),
                reason: "not_down_or_up",
            });
            return Some(reading);
        }
    };
    let message = heartbeat
        .get("msg")
        .and_then(Value::as_str)
        .filter(|message| !message.is_empty())
        .map(str::to_string);
    reading.signals.push(Signal {
        service: service.to_string(),
        state,
        message,
    });
    Some(reading)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_heartbeat_down_or_up_is_a_signal() {
        let body = json!({
            "heartbeat": {"status": 0, "msg": "timeout of 48000ms exceeded"},
            "monitor": {"name": "Mail"},
            "msg": "[Mail] [Down] timeout of 48000ms exceeded",
        });
        assert_eq!(
            read(&body).map(|reading| reading.signals),
            Some(vec![Signal {
                service: "Mail".to_string(),
                state: Reported::Down,
                message: Some("timeout of 48000ms exceeded".to_string()),
            }])
        );
    }

    #[test]
    fn the_test_button_and_a_pending_heartbeat_change_nothing() {
        let test = json!({"heartbeat": null, "monitor": null, "msg": "Statup Testing"});
        assert_eq!(read(&test), Some(Reading::default()));
        let pending = json!({"heartbeat": {"status": 2}, "monitor": {"name": "Mail"}});
        let reading = read(&pending).expect("in this format");
        assert_eq!(reading.signals, []);
        assert_eq!(reading.ignored[0].reason, "not_down_or_up");
    }

    #[test]
    fn another_body_is_not_in_this_format() {
        assert!(read(&json!({"service": "Mail", "state": "down"})).is_none());
        assert!(read(&json!({"heartbeat": {"status": 0}})).is_none());
    }
}
