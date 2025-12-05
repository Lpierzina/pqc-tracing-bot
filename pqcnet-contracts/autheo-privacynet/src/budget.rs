use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{config::BudgetConfig, dp::DpQuery};

#[derive(Debug, Error)]
pub enum BudgetError {
    #[error("privacy budget exhausted for session {session_id}")]
    Exhausted { session_id: u64 },
    #[error("query limit exceeded for session {session_id}")]
    QueryLimit { session_id: u64 },
    #[error("tenant {tenant_id} exceeded daily privacy budget")]
    TenantExhausted { tenant_id: String },
    #[error("tenant {tenant_id} exceeded max queries in rolling window")]
    TenantQueryLimit { tenant_id: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetLedgerSnapshot {
    pub session_id: u64,
    pub epsilon_consumed: f64,
    pub delta_consumed: f64,
    pub queries_seen: u32,
    pub composed_epsilon: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetClaim {
    pub epsilon_remaining: f64,
    pub delta_remaining: f64,
    pub composed_epsilon: f64,
    pub tenant_epsilon_remaining: f64,
    pub tenant_delta_remaining: f64,
    pub tenant_queries_remaining: u32,
}

#[derive(Clone, Debug)]
struct SessionBudget {
    epsilon_spent: f64,
    delta_spent: f64,
    queries: u32,
}

impl SessionBudget {
    fn new() -> Self {
        Self {
            epsilon_spent: 0.0,
            delta_spent: 0.0,
            queries: 0,
        }
    }
}

#[derive(Clone, Debug)]
struct TenantBudget {
    epsilon_spent: f64,
    delta_spent: f64,
    queries: u32,
    window_start_epoch: u64,
}

impl TenantBudget {
    fn new(start: u64) -> Self {
        Self {
            epsilon_spent: 0.0,
            delta_spent: 0.0,
            queries: 0,
            window_start_epoch: start,
        }
    }

    fn reset_if_needed(&mut self, epoch: u64, window: u64) {
        if window == 0 {
            return;
        }
        if epoch.saturating_sub(self.window_start_epoch) >= window {
            *self = TenantBudget::new(epoch);
        }
    }
}

pub struct PrivacyBudgetLedger {
    config: BudgetConfig,
    sessions: HashMap<u64, SessionBudget>,
    tenants: HashMap<String, TenantBudget>,
}

impl PrivacyBudgetLedger {
    pub fn new(config: BudgetConfig) -> Self {
        Self {
            config,
            sessions: HashMap::new(),
            tenants: HashMap::new(),
        }
    }

    pub fn claim(
        &mut self,
        session_id: u64,
        tenant_id: &str,
        chain_epoch: u64,
        query: &DpQuery,
    ) -> Result<BudgetClaim, BudgetError> {
        let state = self
            .sessions
            .entry(session_id)
            .or_insert_with(SessionBudget::new);
        if state.queries >= self.config.max_queries_per_session {
            return Err(BudgetError::QueryLimit { session_id });
        }
        let composed = Self::compose_privacy_loss(state.queries + 1, query);
        let epsilon_total = state.epsilon_spent + query.epsilon;
        let delta_total = state.delta_spent + query.delta;
        if epsilon_total > self.config.session_epsilon || delta_total > self.config.session_delta {
            return Err(BudgetError::Exhausted { session_id });
        }
        let tenant = self
            .tenants
            .entry(tenant_id.to_string())
            .or_insert_with(|| TenantBudget::new(chain_epoch));
        tenant.reset_if_needed(chain_epoch, self.config.tenant_epoch_window);
        if tenant.queries >= self.config.max_queries_per_tenant {
            return Err(BudgetError::TenantQueryLimit {
                tenant_id: tenant_id.to_string(),
            });
        }
        let tenant_epsilon_total = tenant.epsilon_spent + query.epsilon;
        let tenant_delta_total = tenant.delta_spent + query.delta;
        if tenant_epsilon_total > self.config.tenant_daily_epsilon
            || tenant_delta_total > self.config.tenant_daily_delta
        {
            return Err(BudgetError::TenantExhausted {
                tenant_id: tenant_id.to_string(),
            });
        }
        state.epsilon_spent = epsilon_total;
        state.delta_spent = delta_total;
        state.queries += 1;
        tenant.epsilon_spent = tenant_epsilon_total;
        tenant.delta_spent = tenant_delta_total;
        tenant.queries += 1;
        Ok(BudgetClaim {
            epsilon_remaining: (self.config.session_epsilon - epsilon_total).max(0.0),
            delta_remaining: (self.config.session_delta - delta_total).max(0.0),
            composed_epsilon: composed,
            tenant_epsilon_remaining: (self.config.tenant_daily_epsilon - tenant_epsilon_total)
                .max(0.0),
            tenant_delta_remaining: (self.config.tenant_daily_delta - tenant_delta_total).max(0.0),
            tenant_queries_remaining: self
                .config
                .max_queries_per_tenant
                .saturating_sub(tenant.queries),
        })
    }

    pub fn settle(&mut self, session_id: u64) {
        self.sessions
            .entry(session_id)
            .or_insert_with(SessionBudget::new);
    }

    pub fn snapshot(&self, session_id: u64) -> BudgetLedgerSnapshot {
        let state = self
            .sessions
            .get(&session_id)
            .cloned()
            .unwrap_or_else(SessionBudget::new);
        let template = DpQuery::gaussian(
            vec![0u64],
            self.config.session_epsilon.max(1e-8),
            self.config.session_delta.max(1e-8),
            1.0,
        );
        let composed = if state.queries == 0 {
            0.0
        } else {
            Self::compose_privacy_loss(state.queries, &template)
        };
        BudgetLedgerSnapshot {
            session_id,
            epsilon_consumed: state.epsilon_spent,
            delta_consumed: state.delta_spent,
            queries_seen: state.queries,
            composed_epsilon: composed,
        }
    }

    fn compose_privacy_loss(k: u32, query: &DpQuery) -> f64 {
        let sensitivity = query.sensitivity.max(1e-18);
        let epsilon = query.epsilon.max(1e-18);
        let delta = query.delta.clamp(1e-18, 0.999999);
        let k = k as f64;
        let first_term = k * epsilon;
        let sigma = sensitivity;
        let second_term = (2.0 * k * (1.0 / delta).ln()).sqrt() * sigma;
        first_term + second_term
    }
}
