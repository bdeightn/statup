//! The webhook of Prometheus Alertmanager, and of the tools that send its
//! format, Grafana among them: a list of alerts, each `firing` or
//! `resolved`. An alert names its service in a `service` label; one
//! without is left out, as the alerts a tool sends for a test are.

use serde_json::Value;

use super::{Ignored, Reading, Reported, Signal};

/// `None` when the body is not in this format. Known by its shape, not by
/// its `version`: Alertmanager says `4`, Grafana `1`.
pub(super) fn read(body: &Value) -> Option<Reading> {
    let alerts = body.get("alerts")?.as_array()?;
    let shaped = alerts
        .iter()
        .all(|alert| alert.get("status").is_some() && alert.get("labels").is_some());
    if !shaped {
        return None;
    }
    let mut reading = Reading::default();
    for alert in alerts {
        take(alert, &mut reading);
    }
    Some(reading)
}

fn take(alert: &Value, reading: &mut Reading) {
    let service = alert
        .pointer("/labels/service")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|service| !service.is_empty());
    let Some(service) = service else {
        reading.ignored.push(Ignored {
            service: None,
            reason: "no_service_label",
        });
        return;
    };
    let state = match alert.get("status").and_then(Value::as_str) {
        Some("firing") => Reported::Down,
        Some("resolved") => Reported::Up,
        _ => {
            reading.ignored.push(Ignored {
                service: Some(service.to_string()),
                reason: "not_down_or_up",
            });
            return;
        }
    };
    // A resolved alert keeps the words of the problem: they would read
    // wrong on the line that says the service is back.
    let message = alert
        .pointer("/annotations/summary")
        .or_else(|| alert.pointer("/labels/alertname"))
        .and_then(Value::as_str)
        .filter(|_| state == Reported::Down)
        .map(str::to_string);
    reading.signals.push(Signal {
        service: service.to_string(),
        state,
        message,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn each_alert_with_a_service_label_is_a_signal() {
        let body = json!({"alerts": [
            {"status": "firing", "labels": {"alertname": "MailDown", "service": "Mail"}},
            {"status": "resolved", "labels": {"service": "VPN"},
             "annotations": {"summary": "VPN answers"}},
            {"status": "firing", "labels": {"alertname": "Watchdog"}},
        ]});
        let reading = read(&body).expect("in this format");
        assert_eq!(
            reading.signals,
            [
                Signal {
                    service: "Mail".to_string(),
                    state: Reported::Down,
                    message: Some("MailDown".to_string()),
                },
                Signal {
                    service: "VPN".to_string(),
                    state: Reported::Up,
                    message: None,
                },
            ]
        );
        assert_eq!(
            reading.ignored,
            [Ignored {
                service: None,
                reason: "no_service_label",
            }]
        );
    }

    #[test]
    fn another_body_is_not_in_this_format() {
        assert!(read(&json!({"service": "Mail", "state": "down"})).is_none());
        assert!(read(&json!({"alerts": [{"name": "x"}]})).is_none());
    }
}
