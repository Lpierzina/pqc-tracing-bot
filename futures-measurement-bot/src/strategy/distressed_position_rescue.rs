//! Distressed Position Rescue Scanner
//!
//! Goal: given a (typically distressed) short premium vertical spread, enumerate
//! candidate "escape routes" that improve time decay (Theta) and reduce break-even,
//! even if doing so increases capital at risk (widening spreads).
//!
//! This module is intentionally self-contained (no external quant deps).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptionKind {
    Call,
    Put,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BsInputs {
    pub s: f64,     // spot
    pub k: f64,     // strike
    pub t: f64,     // years
    pub r: f64,     // risk-free (cc)
    pub q: f64,     // dividend/borrow (cc)
    pub sigma: f64, // iv (annualized)
    pub kind: OptionKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BsGreeks {
    pub price: f64,          // per share
    pub delta: f64,          // per share
    pub gamma: f64,          // per share
    pub vega: f64,           // per 1.0 vol (i.e. per 100 vol points)
    pub theta_per_year: f64, // per share per year
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerticalSpread {
    pub kind: OptionKind,
    /// The "short" strike of the credit spread (sold).
    pub short_strike: f64,
    /// The "long" strike of the credit spread (bought).
    pub long_strike: f64,
    /// Days to expiration.
    pub dte_days: u32,
    /// Number of 100-share option contracts.
    pub contracts: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpreadInputs {
    pub underlying: f64,
    pub iv: f64, // 0.35 = 35%
    pub r: f64,
    pub q: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpreadMetrics {
    pub theo_credit: f64, // per share (>= 0 means credit)
    pub break_even: f64,  // underlying price
    pub width: f64,
    pub max_profit: f64,      // total dollars (all contracts)
    pub max_loss: f64,        // total dollars (all contracts)
    pub capital_at_risk: f64, // total dollars (all contracts)
    pub net_delta: f64,
    pub net_theta_per_day: f64, // total dollars per day
    pub net_vega: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RescueScanRequest {
    pub symbol: Option<String>,
    pub spread: VerticalSpread,
    pub inputs: SpreadInputs,
    /// Optional override for current/paid credit (per share). If omitted, uses theoretical credit.
    pub current_credit: Option<f64>,
    /// Maximum candidates returned.
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateRoute {
    pub route: String,
    pub spread: VerticalSpread,
    pub metrics: SpreadMetrics,
    pub score: f64,
    pub deltas: CandidateDeltaSummary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateDeltaSummary {
    pub break_even_change: f64, // new - current (negative is better for put credit spread)
    pub theta_per_day_change: f64, // new - current
    pub capital_at_risk_change: f64, // new - current
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RescueScanResponse {
    pub symbol: Option<String>,
    pub current: SpreadMetrics,
    pub candidates: Vec<CandidateRoute>,
}

pub fn scan(req: &RescueScanRequest) -> anyhow::Result<RescueScanResponse> {
    validate_req(req)?;

    let mut base = req.spread;
    base.contracts = base.contracts.max(1);

    let current_metrics_theo = metrics_for_spread(base, req.inputs)?;
    let current_credit = req
        .current_credit
        .unwrap_or(current_metrics_theo.theo_credit);
    let current = metrics_for_spread_with_credit(base, req.inputs, current_credit)?;

    // Candidate axes:
    // - roll out: extend DTE
    // - roll down: move strikes down (puts) / up (calls)
    // - widen: increase width (move long leg further OTM)
    //
    // We'll generate a reasonable grid and then score/rank.
    let dte_grid: [u32; 7] = [
        base.dte_days,
        base.dte_days.saturating_add(7),
        base.dte_days.saturating_add(14),
        base.dte_days.saturating_add(21),
        base.dte_days.saturating_add(28),
        base.dte_days.saturating_add(42),
        base.dte_days.saturating_add(63),
    ];

    // Strike step sizes in dollars. (We intentionally keep this simple and UI-driven.)
    let strike_shifts: [f64; 7] = [0.0, 0.5, 1.0, 2.0, 3.0, 5.0, 7.5];
    let widen_steps: [f64; 6] = [0.0, 0.5, 1.0, 2.0, 3.0, 5.0];

    let mut out: Vec<CandidateRoute> = Vec::new();

    for &dte in &dte_grid {
        if dte == 0 {
            continue;
        }

        for &shift in &strike_shifts {
            for &widen in &widen_steps {
                let candidate = build_candidate(base, dte, shift, widen);
                if !is_valid_vertical(candidate) {
                    continue;
                }

                let metrics = metrics_for_spread(candidate, req.inputs)?;

                // Only keep candidates that have at least *some* plausible rescue intent:
                // - positive theta, OR
                // - improved break-even
                if metrics.net_theta_per_day <= 0.0
                    && !better_break_even(base.kind, metrics.break_even, current.break_even)
                {
                    continue;
                }

                let deltas = CandidateDeltaSummary {
                    break_even_change: metrics.break_even - current.break_even,
                    theta_per_day_change: metrics.net_theta_per_day - current.net_theta_per_day,
                    capital_at_risk_change: metrics.capital_at_risk - current.capital_at_risk,
                };

                let score = score_candidate(base.kind, &current, &metrics);
                let route = describe_route(base, candidate);

                out.push(CandidateRoute {
                    route,
                    spread: candidate,
                    metrics,
                    score,
                    deltas,
                });
            }
        }
    }

    // De-dup by (dte, strikes) keeping best score.
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut deduped: Vec<CandidateRoute> = Vec::with_capacity(out.len());
    for c in out {
        if let Some(prev) = deduped.iter().find(|x| {
            x.spread.dte_days == c.spread.dte_days
                && (x.spread.short_strike - c.spread.short_strike).abs() < 1e-9
                && (x.spread.long_strike - c.spread.long_strike).abs() < 1e-9
                && x.spread.kind == c.spread.kind
        }) {
            // already have best due to sorting
            let _ = prev;
            continue;
        }
        deduped.push(c);
    }

    let limit = req.limit.unwrap_or(50).clamp(1, 250);
    deduped.truncate(limit);

    Ok(RescueScanResponse {
        symbol: req.symbol.clone(),
        current,
        candidates: deduped,
    })
}

fn validate_req(req: &RescueScanRequest) -> anyhow::Result<()> {
    if !(req.inputs.underlying.is_finite() && req.inputs.underlying > 0.0) {
        anyhow::bail!("invalid underlying");
    }
    if !(req.inputs.iv.is_finite() && req.inputs.iv > 0.0 && req.inputs.iv < 5.0) {
        anyhow::bail!("invalid iv");
    }
    if !(req.inputs.r.is_finite() && req.inputs.r > -0.5 && req.inputs.r < 1.0) {
        anyhow::bail!("invalid r");
    }
    if !(req.inputs.q.is_finite() && req.inputs.q > -0.5 && req.inputs.q < 1.0) {
        anyhow::bail!("invalid q");
    }
    if req.spread.dte_days == 0 {
        anyhow::bail!("dte_days must be > 0");
    }
    if req.spread.contracts == 0 {
        anyhow::bail!("contracts must be non-zero");
    }
    if !is_valid_vertical(req.spread) {
        anyhow::bail!("invalid vertical spread (check strikes)");
    }
    if let Some(c) = req.current_credit {
        if !(c.is_finite() && c >= -50.0 && c <= 50.0) {
            anyhow::bail!("invalid current_credit");
        }
    }
    Ok(())
}

fn is_valid_vertical(s: VerticalSpread) -> bool {
    let (k_short, k_long) = (s.short_strike, s.long_strike);
    if !(k_short.is_finite() && k_long.is_finite()) {
        return false;
    }
    if k_short <= 0.0 || k_long <= 0.0 {
        return false;
    }
    match s.kind {
        OptionKind::Put => k_long < k_short,
        OptionKind::Call => k_long > k_short,
    }
}

fn build_candidate(
    base: VerticalSpread,
    dte_days: u32,
    strike_shift: f64,
    widen: f64,
) -> VerticalSpread {
    let mut c = base;
    c.dte_days = dte_days;

    match base.kind {
        OptionKind::Put => {
            // Roll down (puts): move both strikes down.
            c.short_strike = (base.short_strike - strike_shift).max(0.5);
            c.long_strike = (base.long_strike - strike_shift - widen).max(0.5);
        }
        OptionKind::Call => {
            // Roll down (calls): move both strikes up.
            c.short_strike = (base.short_strike + strike_shift).max(0.5);
            c.long_strike = (base.long_strike + strike_shift + widen).max(0.5);
        }
    }

    c
}

fn describe_route(base: VerticalSpread, cand: VerticalSpread) -> String {
    let mut parts: Vec<String> = Vec::new();
    if cand.dte_days != base.dte_days {
        parts.push(format!("roll out to {} DTE", cand.dte_days));
    }
    if (cand.short_strike - base.short_strike).abs() > 1e-9 {
        match base.kind {
            OptionKind::Put => parts.push(format!("roll down short to {:.2}", cand.short_strike)),
            OptionKind::Call => parts.push(format!("roll down short to {:.2}", cand.short_strike)),
        }
    }
    if (cand.long_strike - base.long_strike).abs() > 1e-9 {
        parts.push(format!("widen to long {:.2}", cand.long_strike));
    }
    if parts.is_empty() {
        "no-op".to_string()
    } else {
        parts.join(" + ")
    }
}

fn better_break_even(kind: OptionKind, new_be: f64, cur_be: f64) -> bool {
    match kind {
        OptionKind::Put => new_be < cur_be,
        OptionKind::Call => new_be > cur_be,
    }
}

fn score_candidate(kind: OptionKind, cur: &SpreadMetrics, cand: &SpreadMetrics) -> f64 {
    // Primary objectives:
    // - positive theta
    // - lower break-even for put credit spreads / higher for call credit spreads
    // Allowed tradeoff:
    // - higher capital at risk (widening) is OK but not free.

    let theta = cand.net_theta_per_day;
    let theta_bonus = if theta > 0.0 {
        theta * 500.0
    } else {
        theta * 2500.0
    };

    let be_improvement = match kind {
        OptionKind::Put => cur.break_even - cand.break_even,
        OptionKind::Call => cand.break_even - cur.break_even,
    };
    let be_bonus = be_improvement * 200.0;

    // Risk penalty (small): we accept widening, but prefer efficient improvements.
    let risk_delta = cand.capital_at_risk - cur.capital_at_risk;
    let risk_penalty = risk_delta.max(0.0) * 0.15;

    // Extra penalty if theta isn't positive.
    let theta_gate_penalty = if cand.net_theta_per_day <= 0.0 {
        25_000.0
    } else {
        0.0
    };

    theta_bonus + be_bonus - risk_penalty - theta_gate_penalty
}

pub fn metrics_for_spread(
    spread: VerticalSpread,
    inputs: SpreadInputs,
) -> anyhow::Result<SpreadMetrics> {
    let credit = theoretical_credit(spread, inputs)?;
    metrics_for_spread_with_credit(spread, inputs, credit)
}

pub fn metrics_for_spread_with_credit(
    spread: VerticalSpread,
    inputs: SpreadInputs,
    credit_per_share: f64,
) -> anyhow::Result<SpreadMetrics> {
    let contract_mult = 100.0;
    let n = spread.contracts as f64;
    let width = (spread.short_strike - spread.long_strike).abs();

    let be = match spread.kind {
        OptionKind::Put => spread.short_strike - credit_per_share,
        OptionKind::Call => spread.short_strike + credit_per_share,
    };

    let max_profit = (credit_per_share * contract_mult * n).max(0.0);
    let max_loss = ((width - credit_per_share).max(0.0) * contract_mult * n).max(0.0);
    let capital_at_risk = max_loss;

    let (greeks_short, greeks_long) = theoretical_legs(spread, inputs)?;
    let net_delta = (-1.0 * greeks_short.delta + 1.0 * greeks_long.delta) * contract_mult * n;
    let net_theta_per_day =
        (-1.0 * greeks_short.theta_per_year + 1.0 * greeks_long.theta_per_year) * contract_mult * n
            / 365.0;
    let net_vega = (-1.0 * greeks_short.vega + 1.0 * greeks_long.vega) * contract_mult * n;

    Ok(SpreadMetrics {
        theo_credit: credit_per_share,
        break_even: be,
        width,
        max_profit,
        max_loss,
        capital_at_risk,
        net_delta,
        net_theta_per_day,
        net_vega,
    })
}

fn theoretical_credit(spread: VerticalSpread, inputs: SpreadInputs) -> anyhow::Result<f64> {
    let (short, long) = theoretical_legs(spread, inputs)?;
    // credit = sell short - buy long
    Ok(short.price - long.price)
}

fn theoretical_legs(
    spread: VerticalSpread,
    inputs: SpreadInputs,
) -> anyhow::Result<(BsGreeks, BsGreeks)> {
    let t = (spread.dte_days as f64) / 365.0;
    if t <= 0.0 {
        anyhow::bail!("invalid time to expiry");
    }

    let short = bs_greeks(BsInputs {
        s: inputs.underlying,
        k: spread.short_strike,
        t,
        r: inputs.r,
        q: inputs.q,
        sigma: inputs.iv,
        kind: spread.kind,
    })?;
    let long = bs_greeks(BsInputs {
        s: inputs.underlying,
        k: spread.long_strike,
        t,
        r: inputs.r,
        q: inputs.q,
        sigma: inputs.iv,
        kind: spread.kind,
    })?;
    Ok((short, long))
}

pub fn bs_greeks(i: BsInputs) -> anyhow::Result<BsGreeks> {
    if !(i.s.is_finite() && i.s > 0.0) {
        anyhow::bail!("invalid spot");
    }
    if !(i.k.is_finite() && i.k > 0.0) {
        anyhow::bail!("invalid strike");
    }
    if !(i.t.is_finite() && i.t > 0.0) {
        anyhow::bail!("invalid t");
    }
    if !(i.sigma.is_finite() && i.sigma > 0.0) {
        anyhow::bail!("invalid sigma");
    }

    let sqrt_t = i.t.sqrt();
    let vsqrt = i.sigma * sqrt_t;
    let ln_sk = (i.s / i.k).ln();
    let d1 = (ln_sk + (i.r - i.q + 0.5 * i.sigma * i.sigma) * i.t) / vsqrt;
    let d2 = d1 - vsqrt;

    let nd1 = norm_cdf(d1);
    let nd2 = norm_cdf(d2);
    let nmd1 = norm_cdf(-d1);
    let nmd2 = norm_cdf(-d2);
    let pdf_d1 = norm_pdf(d1);

    let disc_q = (-i.q * i.t).exp();
    let disc_r = (-i.r * i.t).exp();

    let price;
    let delta;
    let theta;

    match i.kind {
        OptionKind::Call => {
            price = i.s * disc_q * nd1 - i.k * disc_r * nd2;
            delta = disc_q * nd1;
            theta = -i.s * disc_q * pdf_d1 * i.sigma / (2.0 * sqrt_t) - i.r * i.k * disc_r * nd2
                + i.q * i.s * disc_q * nd1;
        }
        OptionKind::Put => {
            price = i.k * disc_r * nmd2 - i.s * disc_q * nmd1;
            delta = disc_q * (nd1 - 1.0);
            theta = -i.s * disc_q * pdf_d1 * i.sigma / (2.0 * sqrt_t) + i.r * i.k * disc_r * nmd2
                - i.q * i.s * disc_q * nmd1;
        }
    }

    let gamma = disc_q * pdf_d1 / (i.s * vsqrt);
    let vega = i.s * disc_q * pdf_d1 * sqrt_t;

    Ok(BsGreeks {
        price,
        delta,
        gamma,
        vega,
        theta_per_year: theta,
    })
}

fn norm_pdf(x: f64) -> f64 {
    const INV_SQRT_2PI: f64 = 0.398_942_280_401_432_7;
    INV_SQRT_2PI * (-0.5 * x * x).exp()
}

fn norm_cdf(x: f64) -> f64 {
    // Abramowitz & Stegun 7.1.26 approximation.
    // Max error ~ 7.5e-8.
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.231_641_9 * z);
    let y = 1.0
        - norm_pdf(z)
            * (((((1.330_274_429 * t - 1.821_255_978) * t + 1.781_477_937) * t - 0.356_563_782)
                * t
                + 0.319_381_530)
                * t);
    if sign >= 0.0 {
        y
    } else {
        1.0 - y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_returns_candidates_for_distressed_put_credit_spread() {
        let req = RescueScanRequest {
            symbol: Some("PLTR".to_string()),
            spread: VerticalSpread {
                kind: OptionKind::Put,
                short_strike: 30.0,
                long_strike: 27.0,
                dte_days: 14,
                contracts: 1,
            },
            inputs: SpreadInputs {
                underlying: 27.5, // distressed (below short strike)
                iv: 0.55,
                r: 0.04,
                q: 0.0,
            },
            current_credit: Some(1.10),
            limit: Some(25),
        };

        let resp = scan(&req).expect("scan ok");
        assert!(!resp.candidates.is_empty());
        assert!(resp.current.width > 0.0);
    }
}
