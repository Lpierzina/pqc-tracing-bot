use hdrhistogram::Histogram;

#[derive(Clone, Debug, Default)]
pub struct FillCounters {
    pub filled: u64,
    pub cancelled: u64,
    pub timed_out: u64,
}

impl FillCounters {
    pub fn total_terminal(&self) -> u64 {
        self.filled + self.cancelled + self.timed_out
    }

    pub fn fill_probability(&self) -> f64 {
        let denom = self.total_terminal();
        if denom == 0 {
            return 0.0;
        }
        (self.filled as f64) / (denom as f64)
    }
}

#[derive(Clone, Debug, Default)]
pub struct RejectCounters {
    pub submitted: u64,
    pub rejected: u64,
}

impl RejectCounters {
    pub fn rejection_rate(&self) -> f64 {
        if self.submitted == 0 {
            return 0.0;
        }
        (self.rejected as f64) / (self.submitted as f64)
    }
}

#[derive(Clone, Debug)]
pub struct Histo {
    /// Store values in micro-units (e.g., ms, bps*10, ticks*100) as integers.
    inner: Histogram<u64>,
}

impl Default for Histo {
    fn default() -> Self {
        Self {
            inner: Histogram::new(3).expect("histo"),
        }
    }
}

impl Histo {
    pub fn record(&mut self, v: u64) {
        let _ = self.inner.record(v.max(1));
    }

    pub fn p50(&self) -> u64 {
        self.inner.value_at_quantile(0.50)
    }

    pub fn p95(&self) -> u64 {
        self.inner.value_at_quantile(0.95)
    }

    pub fn p99(&self) -> u64 {
        self.inner.value_at_quantile(0.99)
    }

    pub fn max(&self) -> u64 {
        self.inner.max()
    }

    pub fn count(&self) -> u64 {
        self.inner.len()
    }
}

#[derive(Clone, Debug, Default)]
pub struct BucketStats {
    pub fills: FillCounters,
    pub rejects: RejectCounters,

    /// Adverse slippage in basis points (always positive), where \"adverse\" means worse than reference.
    pub adverse_slippage_bps: Histo,
    /// Favorable slippage in basis points (always positive), where \"favorable\" means better than reference.
    pub favorable_slippage_bps: Histo,

    /// Decision->send, send->ack, send->first_fill, send->last_fill (milliseconds).
    pub latency_decision_to_send_ms: Histo,
    pub latency_send_to_ack_ms: Histo,
    pub latency_send_to_first_fill_ms: Histo,
    pub latency_send_to_last_fill_ms: Histo,

    /// Microstructure response: mid drift at horizons after fill (bps).
    pub mid_drift_100ms_bps: Histo,
    pub mid_drift_1s_bps: Histo,
    pub mid_drift_5s_bps: Histo,

    /// Spread and depth at decision time.
    pub spread_bps_at_decision: Histo,
    pub depth_bid_topn_at_decision: Histo,
    pub depth_ask_topn_at_decision: Histo,
}
