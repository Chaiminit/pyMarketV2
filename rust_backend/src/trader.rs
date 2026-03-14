use crate::finance::{Direction, Order, TradingPair};
use dashmap::DashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static TRADER_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_trader_id() -> u64 {
    TRADER_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Trader represents a market participant
#[derive(Debug)]
pub struct Trader {
    pub id: u64,
    pub name: String,
    pub assets: DashMap<String, Decimal>, // token_name -> amount
    pub orders: RwLock<Vec<u64>>,         // order_ids
    pub cliented_trading_pairs: DashMap<String, ()>,
}

impl Trader {
    pub fn new(name: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            id: next_trader_id(),
            name: name.into(),
            assets: DashMap::new(),
            orders: RwLock::new(Vec::new()),
            cliented_trading_pairs: DashMap::new(),
        })
    }

    /// Add asset to trader
    pub fn add_asset(&self, token_name: impl Into<String>, amount: Decimal) {
        let token_name = token_name.into();
        self.assets
            .entry(token_name)
            .and_modify(|a| *a += amount)
            .or_insert(amount);
    }

    /// Get asset value
    pub fn get_asset(&self, token_name: &str) -> Decimal {
        self.assets.get(token_name).map(|a| *a).unwrap_or(Decimal::ZERO)
    }

    /// Submit a limit order
    pub fn submit(
        &self,
        trading_pair: &Arc<TradingPair>,
        direction: Direction,
        price: Decimal,
        volume: Decimal,
    ) -> Option<Order> {
        self.cliented_trading_pairs.insert(trading_pair.id.clone(), ());

        // Ensure assets exist
        if !self.assets.contains_key(&trading_pair.quote_token.name) {
            self.assets
                .insert(trading_pair.quote_token.name.clone(), Decimal::ZERO);
        }
        if !self.assets.contains_key(&trading_pair.base_token.name) {
            self.assets
                .insert(trading_pair.base_token.name.clone(), Decimal::ZERO);
        }

        // Check if assets are sufficient
        match direction {
            Direction::Buy => {
                let required = price * volume;
                let quote_amount = self.get_asset(&trading_pair.quote_token.name);
                if quote_amount < required {
                    return None;
                }

                // Freeze assets
                if let Some(mut amount) = self.assets.get_mut(&trading_pair.quote_token.name) {
                    *amount -= required;
                }
            }
            Direction::Sell => {
                let base_amount = self.get_asset(&trading_pair.base_token.name);
                if base_amount < volume {
                    return None;
                }

                // Freeze assets
                if let Some(mut amount) = self.assets.get_mut(&trading_pair.base_token.name) {
                    *amount -= volume;
                }
            }
        }

        // Create order
        let mut order = Order::new(self.id, &trading_pair.id, direction, price, volume);

        // Set frozen amount
        order.frozen_amount = match direction {
            Direction::Buy => price * volume,
            Direction::Sell => volume,
        };

        // Add to trader's orders
        self.orders.write().push(order.id);

        // Submit to trading pair
        trading_pair.recv(order.clone());

        Some(order)
    }

    /// Submit a market order
    pub fn submit_market(
        &self,
        trading_pair: &Arc<TradingPair>,
        direction: Direction,
        volume: Decimal,
    ) -> Option<Vec<crate::finance::TradeEntry>> {
        self.cliented_trading_pairs.insert(trading_pair.id.clone(), ());

        // Ensure assets exist
        if !self.assets.contains_key(&trading_pair.quote_token.name) {
            self.assets
                .insert(trading_pair.quote_token.name.clone(), Decimal::ZERO);
        }
        if !self.assets.contains_key(&trading_pair.base_token.name) {
            self.assets
                .insert(trading_pair.base_token.name.clone(), Decimal::ZERO);
        }

        // Check if assets are sufficient
        match direction {
            Direction::Buy => {
                // For market buy, we need to estimate the cost
                // Get current price as estimate
                let current_price = *trading_pair.price.read();
                let estimated_cost = current_price * volume * Decimal::from(2); // 2x buffer for safety
                let quote_amount = self.get_asset(&trading_pair.quote_token.name);
                if quote_amount < estimated_cost {
                    return None;
                }
            }
            Direction::Sell => {
                let base_amount = self.get_asset(&trading_pair.base_token.name);
                if base_amount < volume {
                    return None;
                }
            }
        }

        // Execute market order
        trading_pair.recv_market(self.id, direction, volume)
    }

    /// Cancel an order
    pub fn cancel_order(&self, order_id: u64, trading_pair: &Arc<TradingPair>) -> Option<Order> {
        // Remove from trader's orders
        {
            let mut orders = self.orders.write();
            if let Some(pos) = orders.iter().position(|&id| id == order_id) {
                orders.remove(pos);
            }
        }

        // Cancel from trading pair
        trading_pair.cancel_order(order_id)
    }

    /// Get total value in quote token
    pub fn get_total_value(&self, quote_token: &str) -> Decimal {
        let mut total = Decimal::ZERO;

        // Sum all assets
        for entry in self.assets.iter() {
            total += *entry.value();
        }

        // Sum frozen amounts in orders
        // Note: In a real implementation, we'd need to track orders per trader
        // This is simplified

        total
    }

    /// Get trader info for serialization
    pub fn get_info(&self) -> TraderInfo {
        let assets: std::collections::HashMap<String, Decimal> = self
            .assets
            .iter()
            .map(|e| (e.key().clone(), *e.value()))
            .collect();

        TraderInfo {
            id: self.id,
            name: self.name.clone(),
            assets,
            orders: self.orders.read().clone(),
        }
    }
}

/// Serializable trader info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderInfo {
    pub id: u64,
    pub name: String,
    pub assets: std::collections::HashMap<String, Decimal>,
    pub orders: Vec<u64>,
}
