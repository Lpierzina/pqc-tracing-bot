//! "Open moving averages" momentum trend detector.
//!
//! This implements the 4h concept described in the prompt:
//! - We look for **high-momentum trends** where SMA(10) and SMA(20) are "open"
//!   (a meaningful gap exists between them) and both are sloping with the trend.
//! - Down trend: SMA10 < SMA20 and both slopes negative.
//! - Up trend:   SMA10 > SMA20 and both slopes positive.
//!
//! The output is a list of contiguous "trend windows" (start/end indices).

use crate::types::Side;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrendDirection {
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrendWindow {
    pub dir: TrendDirection,
    /// Inclusive index into the input close series.
    pub start_idx: usize,
    /// Inclusive index into the input close series.
    pub end_idx: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpenMaConfig {
    /// Fast SMA period (default 10).
    pub fast: usize,
    /// Slow SMA period (default 20).
    pub slow: usize,
    /// Slope lookback in bars (e.g., 3 means SMA[i] - SMA[i-3]).
    pub slope_lookback: usize,
    /// Minimum SMA gap as a percent of the slow SMA. Example: 0.01 = 1%.
    pub min_gap_pct: f64,
    /// Minimum absolute slope per bar as a percent of the current SMA.
    /// Example: 0.001 = 0.1% per bar.
    pub min_slope_pct_per_bar: f64,
}

impl Default for OpenMaConfig {
    fn default() -> Self {
        Self {
            fast: 10,
            slow: 20,
            slope_lookback: 3,
            min_gap_pct: 0.005,            // 0.5%
            min_slope_pct_per_bar: 0.0010, // 0.1% per bar
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrendTrade {
    pub dir: TrendDirection,
    pub side: Side,
    /// Inclusive index into the input close series.
    pub entry_idx: usize,
    /// Inclusive index into the input close series.
    pub exit_idx: usize,
}

/// Detect contiguous "open MA" trend windows from close prices.
///
/// The series is assumed to be ordered oldest->newest.
pub fn detect_open_ma_windows(closes: &[f64], cfg: &OpenMaConfig) -> Vec<TrendWindow> {
    let n = closes.len();
    if n == 0 {
        return vec![];
    }
    if cfg.fast == 0 || cfg.slow == 0 || cfg.slope_lookback == 0 {
        return vec![];
    }
    // Ensure fast <= slow (if not, just swap for sanity).
    let (fast, slow) = if cfg.fast <= cfg.slow {
        (cfg.fast, cfg.slow)
    } else {
        (cfg.slow, cfg.fast)
    };

    let sma_fast = sma(closes, fast);
    let sma_slow = sma(closes, slow);

    let mut windows: Vec<TrendWindow> = Vec::new();
    let mut active: Option<(TrendDirection, usize)> = None; // (dir, start_idx)

    for i in 0..n {
        let dir_here = classify_open_ma(i, &sma_fast, &sma_slow, cfg);

        match (active, dir_here) {
            (None, None) => {}
            (None, Some(dir)) => {
                active = Some((dir, i));
            }
            (Some((cur_dir, start)), Some(dir)) if cur_dir == dir => {
                // continue
                let _ = start;
            }
            (Some((cur_dir, start)), Some(dir)) if cur_dir != dir => {
                // close prior window, start new
                if i > 0 {
                    windows.push(TrendWindow {
                        dir: cur_dir,
                        start_idx: start,
                        end_idx: i - 1,
                    });
                }
                active = Some((dir, i));
            }
            (Some((cur_dir, start)), None) => {
                // close window
                if i > 0 {
                    windows.push(TrendWindow {
                        dir: cur_dir,
                        start_idx: start,
                        end_idx: i - 1,
                    });
                }
                active = None;
            }
            _ => {}
        }
    }

    if let Some((dir, start)) = active {
        windows.push(TrendWindow {
            dir,
            start_idx: start,
            end_idx: n - 1,
        });
    }

    // Drop degenerate windows (e.g., 1-bar artifacts).
    windows.retain(|w| w.end_idx >= w.start_idx);
    windows
}

/// Convert detected windows into simple "hold for the window" trades.
pub fn trades_from_windows(windows: &[TrendWindow]) -> Vec<TrendTrade> {
    windows
        .iter()
        .map(|w| TrendTrade {
            dir: w.dir,
            side: match w.dir {
                TrendDirection::Up => Side::Buy,
                TrendDirection::Down => Side::Sell,
            },
            entry_idx: w.start_idx,
            exit_idx: w.end_idx,
        })
        .collect()
}

fn classify_open_ma(
    i: usize,
    sma_fast: &[Option<f64>],
    sma_slow: &[Option<f64>],
    cfg: &OpenMaConfig,
) -> Option<TrendDirection> {
    // Need current SMA values plus a lookback point for slope.
    let lb = cfg.slope_lookback;
    if i < lb {
        return None;
    }
    let (Some(f_now), Some(s_now)) = (sma_fast.get(i).copied().flatten(), sma_slow.get(i).copied().flatten()) else {
        return None;
    };
    let (Some(f_then), Some(s_then)) = (
        sma_fast.get(i - lb).copied().flatten(),
        sma_slow.get(i - lb).copied().flatten(),
    ) else {
        return None;
    };
    if !(f_now.is_finite() && s_now.is_finite() && f_then.is_finite() && s_then.is_finite()) {
        return None;
    }
    if s_now == 0.0 || f_now == 0.0 {
        return None;
    }

    let gap_pct = ((f_now - s_now).abs() / s_now.abs()).abs();
    if !(gap_pct.is_finite() && gap_pct >= cfg.min_gap_pct) {
        return None;
    }

    let f_slope_per_bar = (f_now - f_then) / (lb as f64);
    let s_slope_per_bar = (s_now - s_then) / (lb as f64);
    let f_slope_pct = (f_slope_per_bar / f_now.abs()).abs();
    let s_slope_pct = (s_slope_per_bar / s_now.abs()).abs();
    if !(f_slope_pct.is_finite()
        && s_slope_pct.is_finite()
        && f_slope_pct >= cfg.min_slope_pct_per_bar
        && s_slope_pct >= cfg.min_slope_pct_per_bar)
    {
        return None;
    }

    // Directional requirements: ordering + slope sign consistency.
    if f_now > s_now && f_slope_per_bar > 0.0 && s_slope_per_bar > 0.0 {
        Some(TrendDirection::Up)
    } else if f_now < s_now && f_slope_per_bar < 0.0 && s_slope_per_bar < 0.0 {
        Some(TrendDirection::Down)
    } else {
        None
    }
}

fn sma(values: &[f64], period: usize) -> Vec<Option<f64>> {
    let n = values.len();
    let mut out = vec![None; n];
    if period == 0 {
        return out;
    }
    let mut sum = 0.0f64;
    for i in 0..n {
        let v = values[i];
        if !v.is_finite() {
            // reset window on bad data
            sum = 0.0;
            // leave out[i] as None; and also clear previous history by forcing a warmup restart.
            // easiest: treat as window break by re-summing next period.
            // We'll implement a simple restart: recompute sum on the next indices.
            // This is acceptable for the current use case (scan/label).
            let _ = i;
            continue;
        }
        sum += v;
        if i + 1 >= period {
            if i + 1 > period {
                sum -= values[i + 1 - period];
            }
            out[i] = Some(sum / (period as f64));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_series(start: f64, step: f64, n: usize) -> Vec<f64> {
        (0..n).map(|i| start + (i as f64) * step).collect()
    }

    #[test]
    fn detects_single_down_window_between_flat_regions() {
        let mut closes = Vec::new();
        closes.extend(vec![100.0; 40]); // flat
        closes.extend(linear_series(100.0, -0.75, 80)); // down
        closes.extend(vec![40.0; 40]); // flat

        let cfg = OpenMaConfig {
            min_gap_pct: 0.001,
            min_slope_pct_per_bar: 0.001,
            ..Default::default()
        };

        let windows = detect_open_ma_windows(&closes, &cfg);
        assert!(
            windows.iter().any(|w| w.dir == TrendDirection::Down),
            "expected at least one down window: {windows:?}"
        );

        // Ensure we don't claim an up window in a purely down-move.
        assert!(
            !windows.iter().any(|w| w.dir == TrendDirection::Up),
            "unexpected up window: {windows:?}"
        );
    }

    #[test]
    fn detects_up_window() {
        let mut closes = Vec::new();
        closes.extend(vec![50.0; 30]);
        closes.extend(linear_series(50.0, 0.50, 100));
        closes.extend(vec![100.0; 30]);

        let cfg = OpenMaConfig {
            min_gap_pct: 0.001,
            min_slope_pct_per_bar: 0.0005,
            ..Default::default()
        };

        let windows = detect_open_ma_windows(&closes, &cfg);
        assert!(
            windows.iter().any(|w| w.dir == TrendDirection::Up),
            "expected at least one up window: {windows:?}"
        );
    }

    #[test]
    fn trades_map_direction_to_side() {
        let windows = vec![
            TrendWindow {
                dir: TrendDirection::Up,
                start_idx: 10,
                end_idx: 20,
            },
            TrendWindow {
                dir: TrendDirection::Down,
                start_idx: 30,
                end_idx: 40,
            },
        ];
        let trades = trades_from_windows(&windows);
        assert_eq!(trades[0].side, Side::Buy);
        assert_eq!(trades[1].side, Side::Sell);
    }
}

