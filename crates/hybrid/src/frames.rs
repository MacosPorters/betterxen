//! Frame-time statistics from a list of frame intervals in milliseconds (issue #145).
//!
//! Percentiles use the nearest-rank method on the sorted intervals: the p-th percentile is
//! the value at rank ceil(p/100 * n) (1-based). "1 % low FPS" is 1000 divided by the 99th
//! percentile interval. These definitions are fixed here so reports are comparable; other
//! tools define lows differently (for example as the mean of the worst 1 %).

use crate::json::Json;

#[derive(Debug, Clone, PartialEq)]
pub struct FrameStats {
    pub frames: usize,
    pub total_ms: f64,
    pub mean_ms: f64,
    pub avg_fps: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub p999_ms: f64,
    pub low_1pct_fps: f64,
    pub low_01pct_fps: f64,
    /// Frames longer than twice the median interval.
    pub stutters: usize,
}

#[derive(Debug, PartialEq)]
pub enum FrameError {
    Empty,
    Invalid { index: usize, value: f64 },
    Parse { line: usize, text: String },
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Empty => write!(f, "no frame intervals"),
            FrameError::Invalid { index, value } => {
                write!(
                    f,
                    "frame {index}: interval {value} is not a positive finite number of milliseconds"
                )
            }
            FrameError::Parse { line, text } => {
                write!(f, "line {line}: cannot parse {text:?} as milliseconds")
            }
        }
    }
}

impl std::error::Error for FrameError {}

fn nearest_rank(sorted: &[f64], pct: f64) -> f64 {
    let n = sorted.len();
    let rank = ((pct / 100.0) * n as f64).ceil().max(1.0) as usize;
    sorted[rank.min(n) - 1]
}

pub fn analyze(intervals_ms: &[f64]) -> Result<FrameStats, FrameError> {
    if intervals_ms.is_empty() {
        return Err(FrameError::Empty);
    }
    for (index, &value) in intervals_ms.iter().enumerate() {
        if !value.is_finite() || value <= 0.0 {
            return Err(FrameError::Invalid { index, value });
        }
    }
    let mut sorted = intervals_ms.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let total: f64 = intervals_ms.iter().sum();
    let n = sorted.len();
    let p50 = nearest_rank(&sorted, 50.0);
    let p99 = nearest_rank(&sorted, 99.0);
    let p999 = nearest_rank(&sorted, 99.9);
    Ok(FrameStats {
        frames: n,
        total_ms: total,
        mean_ms: total / n as f64,
        avg_fps: n as f64 / (total / 1000.0),
        min_ms: sorted[0],
        max_ms: sorted[n - 1],
        p50_ms: p50,
        p95_ms: nearest_rank(&sorted, 95.0),
        p99_ms: p99,
        p999_ms: p999,
        low_1pct_fps: 1000.0 / p99,
        low_01pct_fps: 1000.0 / p999,
        stutters: intervals_ms.iter().filter(|&&v| v > 2.0 * p50).count(),
    })
}

/// Parse one interval per line; blank lines and lines starting with `#` are skipped.
pub fn parse_intervals(text: &str) -> Result<Vec<f64>, FrameError> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let v: f64 = t.parse().map_err(|_| FrameError::Parse {
            line: i + 1,
            text: t.to_string(),
        })?;
        out.push(v);
    }
    Ok(out)
}

impl FrameStats {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("frames", Json::Num(self.frames as f64)),
            ("total_ms", Json::Num(self.total_ms)),
            ("mean_ms", Json::Num(self.mean_ms)),
            ("avg_fps", Json::Num(self.avg_fps)),
            ("min_ms", Json::Num(self.min_ms)),
            ("max_ms", Json::Num(self.max_ms)),
            ("p50_ms", Json::Num(self.p50_ms)),
            ("p95_ms", Json::Num(self.p95_ms)),
            ("p99_ms", Json::Num(self.p99_ms)),
            ("p999_ms", Json::Num(self.p999_ms)),
            ("low_1pct_fps", Json::Num(self.low_1pct_fps)),
            ("low_0_1pct_fps", Json::Num(self.low_01pct_fps)),
            ("stutters_over_2x_median", Json::Num(self.stutters as f64)),
            ("percentile_method", Json::str("nearest-rank")),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values below are worked out by hand, not produced by analyze().
    #[test]
    fn known_answer_ten_frames() {
        // Sorted: 10,10,10,10,10,10,10,10,20,100. Total 200 ms, n = 10.
        let v = [10.0, 10.0, 10.0, 10.0, 20.0, 10.0, 10.0, 10.0, 100.0, 10.0];
        let s = analyze(&v).unwrap();
        assert_eq!(s.frames, 10);
        assert_eq!(s.total_ms, 200.0);
        assert_eq!(s.mean_ms, 20.0);
        assert_eq!(s.avg_fps, 50.0);
        assert_eq!(s.p50_ms, 10.0); // rank 5
        assert_eq!(s.p95_ms, 100.0); // rank ceil(9.5) = 10
        assert_eq!(s.p99_ms, 100.0);
        assert_eq!(s.low_1pct_fps, 10.0);
        assert_eq!(s.stutters, 1); // threshold is 2 * 10 = 20; 20 is not > 20, only 100 is.
    }

    #[test]
    fn stutter_threshold_is_strict() {
        let s = analyze(&[10.0, 10.0, 10.0, 20.0, 21.0]).unwrap();
        assert_eq!(s.stutters, 1);
    }

    #[test]
    fn one_frame_per_second() {
        let s = analyze(&[1000.0, 1000.0]).unwrap();
        assert_eq!(s.avg_fps, 1.0);
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(analyze(&[]), Err(FrameError::Empty));
        assert!(matches!(
            analyze(&[1.0, 0.0]),
            Err(FrameError::Invalid { index: 1, .. })
        ));
        assert!(matches!(
            analyze(&[f64::NAN]),
            Err(FrameError::Invalid { .. })
        ));
        assert!(matches!(analyze(&[-3.0]), Err(FrameError::Invalid { .. })));
    }

    #[test]
    fn parses_text() {
        assert_eq!(
            parse_intervals("# c\n16.5\n\n 8 \n").unwrap(),
            vec![16.5, 8.0]
        );
        assert_eq!(
            parse_intervals("1\nabc\n"),
            Err(FrameError::Parse {
                line: 2,
                text: "abc".into()
            })
        );
    }
}
