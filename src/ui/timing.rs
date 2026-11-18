//! Client-side observations, not estimates of provider-internal thinking or network phases.
use crate::agent::AgentEvent;
use std::time::{Duration, Instant};
#[derive(Default)]
pub struct Timing {
    started: Option<Instant>,
    finished: Option<Instant>,
    first_output: Option<Duration>,
    provider_ms: u64,
    tools_ms: u64,
    approval_ms: u64,
    requests: u64,
    retries: u64,
    render_ms: u64,
}
impl Timing {
    pub fn start(&mut self) {
        *self = Self {
            started: Some(Instant::now()),
            ..Self::default()
        };
    }
    pub fn render(&mut self, elapsed: Duration) {
        if self.started.is_some() && self.finished.is_none() {
            self.render_ms = self.render_ms.saturating_add(elapsed.as_millis() as u64);
        }
    }
    pub fn observe(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::Timing {
                stage,
                duration_ms,
                attempt,
            } => match *stage {
                "provider" => {
                    self.provider_ms = self.provider_ms.saturating_add(*duration_ms);
                    self.requests += 1;
                    if *attempt > 1 {
                        self.retries += 1;
                    }
                }
                "approval" => self.approval_ms = self.approval_ms.saturating_add(*duration_ms),
                "tools" => self.tools_ms = self.tools_ms.saturating_add(*duration_ms),
                _ => {}
            },
            AgentEvent::StreamDelta(text) | AgentEvent::OutputText(text)
                if !text.is_empty() && self.first_output.is_none() =>
            {
                self.first_output = self.started.map(|s| s.elapsed())
            }
            AgentEvent::Done | AgentEvent::Cancelled | AgentEvent::Error(_) => {
                self.finished = Some(Instant::now())
            }
            _ => {}
        }
    }
    pub fn rows(&self) -> Vec<(String, String)> {
        let format = |ms: u64| crate::ui::helper::elapsed_text(ms, true);
        let total = self.started.map(|s| {
            self.finished
                .unwrap_or_else(Instant::now)
                .saturating_duration_since(s)
                .as_millis() as u64
        });
        vec![
            (
                "Turn elapsed".into(),
                total
                    .map(&format)
                    .unwrap_or_else(|| "No turn recorded".into()),
            ),
            (
                "First visible output".into(),
                self.first_output
                    .map(|d| format(d.as_millis() as u64))
                    .unwrap_or_else(|| "Not observed".into()),
            ),
            (
                "Provider requests".into(),
                format!(
                    "{} attempts · {} retries · {}",
                    self.requests,
                    self.retries,
                    format(self.provider_ms)
                ),
            ),
            ("Tool execution".into(), format(self.tools_ms)),
            ("Approval waits".into(), format(self.approval_ms)),
            (
                "Other turn time".into(),
                total
                    .map(|t| {
                        format(
                            t.saturating_sub(
                                self.provider_ms
                                    .saturating_add(self.tools_ms)
                                    .saturating_add(self.approval_ms),
                            ),
                        )
                    })
                    .unwrap_or_else(|| "—".into()),
            ),
            ("UI round trips".into(), format(self.render_ms)),
        ]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_measured_attempts_and_freezes_after_completion() {
        let mut timing = Timing::default();
        timing.start();
        timing.observe(&AgentEvent::Timing {
            stage: "provider",
            duration_ms: 200,
            attempt: 1,
        });
        timing.observe(&AgentEvent::Timing {
            stage: "provider",
            duration_ms: 300,
            attempt: 2,
        });
        timing.observe(&AgentEvent::Timing {
            stage: "tools",
            duration_ms: 100,
            attempt: 0,
        });
        timing.observe(&AgentEvent::StreamDelta("hello".into()));
        timing.observe(&AgentEvent::Done);
        let frozen = timing.rows();
        timing.render(Duration::from_secs(1));
        assert_eq!(frozen, timing.rows());
        assert_eq!(timing.requests, 2);
        assert_eq!(timing.retries, 1);
        assert_eq!(timing.provider_ms, 500);
        assert_eq!(timing.tools_ms, 100);
        assert!(timing.first_output.is_some());
    }
}
