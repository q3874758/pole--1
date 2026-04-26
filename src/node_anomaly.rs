use crate::node_pipeline::ActivitySample;
use crate::primitives::{ActivitySourceKind, UnixMillis};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SampleAnomalyKind {
    FutureTimestamp,
    ZeroConfidence,
    SuspiciousPlayerJump,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleAnomaly {
    pub sample_index: usize,
    pub kind: SampleAnomalyKind,
}

pub fn detect_sample_anomalies(
    samples: &[ActivitySample],
    now_millis: UnixMillis,
) -> Vec<SampleAnomaly> {
    let mut anomalies = Vec::new();
    for (index, sample) in samples.iter().enumerate() {
        if sample.observed_at_millis > now_millis {
            anomalies.push(SampleAnomaly {
                sample_index: index,
                kind: SampleAnomalyKind::FutureTimestamp,
            });
        }
        if sample.source_confidence_ppm == 0 {
            anomalies.push(SampleAnomaly {
                sample_index: index,
                kind: SampleAnomalyKind::ZeroConfidence,
            });
        }
        if index > 0 {
            let previous = &samples[index - 1];
            if previous.app_id == sample.app_id
                && previous.source_kind == sample.source_kind
                && previous.source_kind != ActivitySourceKind::Community
                && sample.observed_players > previous.observed_players.saturating_mul(10)
            {
                anomalies.push(SampleAnomaly {
                    sample_index: index,
                    kind: SampleAnomalyKind::SuspiciousPlayerJump,
                });
            }
        }
    }
    anomalies
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_pipeline::ActivitySample;

    #[test]
    fn detects_basic_sample_anomalies() {
        let samples = vec![
            ActivitySample::new(730, 100, 100, "{}", ActivitySourceKind::Steam, 1_000_000),
            ActivitySample::new(730, 2_000, 101, "{}", ActivitySourceKind::Steam, 1_000_000),
            ActivitySample::new(730, 50, 500, "{}", ActivitySourceKind::Community, 0),
        ];
        let anomalies = detect_sample_anomalies(&samples, 200);
        assert_eq!(anomalies.len(), 3);
    }
}
