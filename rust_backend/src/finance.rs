use dashmap::DashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Token represents a tradable asset
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub name: String,
}

impl Token {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
        }
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Order direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Buy,
    Sell,
}

impl Direction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Direction::Buy => "buy",
            Direction::Sell => "sell",
        }
    }
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    Active,
    Filled,
    Cancelled,
}

/// Order ID generator
static ORDER_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_order_id() -> u64 {
    ORDER_ID_COUNTER.fetch_add(1, AtomicOrdering::SeqCst)
}

/// Order represents a trade order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: u64,
    pub submitter_id: u64,
    pub trading_pair_id: String,
    pub direction: Direction,
    pub price: Decimal,
    pub total_volume: Decimal,
    pub executed_volume: Decimal,
    pub frozen_amount: Decimal,
    pub submit_time: u64, // timestamp in milliseconds
    pub status: OrderStatus,
}

impl Order {
    pub fn new(
        submitter_id: u64,
        trading_pair_id: impl Into<String>,
        direction: Direction,
        price: Decimal,
        volume: Decimal,
    ) -> Self {
        let submit_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            id: next_order_id(),
            submitter_id,
            trading_pair_id: trading_pair_id.into(),
            direction,
            price,
            total_volume: volume,
            executed_volume: Decimal::ZERO,
            frozen_amount: Decimal::ZERO,
            submit_time,
            status: OrderStatus::Active,
        }
    }

    pub fn remaining_volume(&self) -> Decimal {
        self.total_volume - self.executed_volume
    }

    pub fn is_filled(&self) -> bool {
        self.executed_volume >= self.total_volume
    }
}

/// Priority wrapper for buy orders (max-heap by price, min-heap by time)
#[derive(Clone)]
pub struct BuyOrderPriority {
    pub price: Decimal,
    pub submit_time: u64,
    pub order_id: u64,
}

impl Eq for BuyOrderPriority {}

impl PartialEq for BuyOrderPriority {
    fn eq(&self, other: &Self) -> bool {
        self.order_id == other.order_id
    }
}

impl Ord for BuyOrderPriority {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher price first, then earlier time
        other
            .price
            .cmp(&self.price)
            .then_with(|| self.submit_time.cmp(&other.submit_time))
    }
}

impl PartialOrd for BuyOrderPriority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Priority wrapper for sell orders (min-heap by price, min-heap by time)
#[derive(Clone)]
pub struct SellOrderPriority {
    pub price: Decimal,
    pub submit_time: u64,
    pub order_id: u64,
}

impl Eq for SellOrderPriority {}

impl PartialEq for SellOrderPriority {
    fn eq(&self, other: &Self) -> bool {
        self.order_id == other.order_id
    }
}

impl Ord for SellOrderPriority {
    fn cmp(&self, other: &Self) -> Ordering {
        // Lower price first, then earlier time
        self.price
            .cmp(&other.price)
            .then_with(|| self.submit_time.cmp(&other.submit_time))
    }
}

impl PartialOrd for SellOrderPriority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Trade log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeEntry {
    pub timestamp: u64,
    pub price: Decimal,
    pub volume: Decimal,
}

/// TradingPair manages order book and matching
#[derive(Debug)]
pub struct TradingPair {
    pub id: String,
    pub base_token: Token,
    pub quote_token: Token,
    pub price: RwLock<Decimal>,
    pub buy_orders: RwLock<BinaryHeap<BuyOrderPriority>>,
    pub sell_orders: RwLock<BinaryHeap<SellOrderPriority>>,
    pub orders: DashMap<u64, Order>, // order_id -> Order
    pub trade_log: RwLock<Vec<TradeEntry>>,
    pub clients: DashMap<u64, ()>, // trader_id -> ()
}

impl TradingPair {
    pub fn new(base_token: Token, quote_token: Token, initial_price: Decimal) -> Arc<Self> {
        Arc::new(Self {
            id: format!("{}/{}", base_token.name, quote_token.name),
            base_token,
            quote_token,
            price: RwLock::new(initial_price),
            buy_orders: RwLock::new(BinaryHeap::new()),
            sell_orders: RwLock::new(BinaryHeap::new()),
            orders: DashMap::new(),
            trade_log: RwLock::new(Vec::new()),
            clients: DashMap::new(),
        })
    }

    /// Add a limit order
    pub fn recv(&self, order: Order) {
        let order_id = order.id;
        let priority = match order.direction {
            Direction::Buy => {
                let priority = BuyOrderPriority {
                    price: order.price,
                    submit_time: order.submit_time,
                    order_id,
                };
                self.buy_orders.write().push(priority);
                None
            }
            Direction::Sell => {
                let priority = SellOrderPriority {
                    price: order.price,
                    submit_time: order.submit_time,
                    order_id,
                };
                self.sell_orders.write().push(priority);
                None
            }
        };

        self.orders.insert(order_id, order);
        drop(priority);

        // Try to match orders
        self.update();
    }

    /// Process market order
    pub fn recv_market(
        &self,
        submitter_id: u64,
        direction: Direction,
        volume: Decimal,
    ) -> Option<Vec<TradeEntry>> {
        self.clients.insert(submitter_id, ());

        let mut trades = Vec::new();
        let mut remaining_volume = volume;

        match direction {
            Direction::Buy => {
                // Match against sell orders
                while remaining_volume > Decimal::ZERO {
                    let sell_order_opt = {
                        let heap = self.sell_orders.read();
                        heap.peek().cloned()
                    };

                    if let Some(sell_priority) = sell_order_opt {
                        let mut sell_order = self.orders.get(&sell_priority.order_id)?;

                        let match_volume = remaining_volume.min(sell_order.remaining_volume());
                        let match_price = sell_order.price;

                        // Execute trade
                        let trade = TradeEntry {
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64,
                            price: match_price,
                            volume: match_volume,
                        };

                        // Update order
                        sell_order.executed_volume += match_volume;
                        sell_order.frozen_amount -= match_volume;

                        // Update price
                        *self.price.write() = match_price;

                        // Record trade
                        self.trade_log.write().push(trade.clone());
                        trades.push(trade);

                        remaining_volume -= match_volume;

                        // Check if sell order is filled
                        if sell_order.is_filled() {
                            sell_order.status = OrderStatus::Filled;
                            drop(sell_order);
                            self.orders.remove(&sell_priority.order_id);
                            self.sell_orders.write().pop();
                        }
                    } else {
                        break;
                    }
                }
            }
            Direction::Sell => {
                // Match against buy orders
                while remaining_volume > Decimal::ZERO {
                    let buy_order_opt = {
                        let heap = self.buy_orders.read();
                        heap.peek().cloned()
                    };

                    if let Some(buy_priority) = buy_order_opt {
                        let mut buy_order = self.orders.get(&buy_priority.order_id)?;

                        let match_volume = remaining_volume.min(buy_order.remaining_volume());
                        let match_price = buy_order.price;

                        // Execute trade
                        let trade = TradeEntry {
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64,
                            price: match_price,
                            volume: match_volume,
                        };

                        // Update order
                        buy_order.executed_volume += match_volume;
                        buy_order.frozen_amount -= match_volume * match_price;

                        // Update price
                        *self.price.write() = match_price;

                        // Record trade
                        self.trade_log.write().push(trade.clone());
                        trades.push(trade);

                        remaining_volume -= match_volume;

                        // Check if buy order is filled
                        if buy_order.is_filled() {
                            buy_order.status = OrderStatus::Filled;
                            drop(buy_order);
                            self.orders.remove(&buy_priority.order_id);
                            self.buy_orders.write().pop();
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        if trades.is_empty() {
            None
        } else {
            Some(trades)
        }
    }

    /// Match orders in the order book
    pub fn update(&self) {
        loop {
            let (buy_priority, sell_priority) = {
                let buy_heap = self.buy_orders.read();
                let sell_heap = self.sell_orders.read();

                let buy_opt = buy_heap.peek().cloned();
                let sell_opt = sell_heap.peek().cloned();

                match (buy_opt, sell_opt) {
                    (Some(b), Some(s)) => (b, s),
                    _ => break,
                }
            };

            // Check price cross
            if buy_priority.price < sell_priority.price {
                break;
            }

            // Get orders
            let mut buy_order = match self.orders.get(&buy_priority.order_id) {
                Some(o) => o,
                None => {
                    self.buy_orders.write().pop();
                    continue;
                }
            };

            let mut sell_order = match self.orders.get(&sell_priority.order_id) {
                Some(o) => o,
                None => {
                    self.sell_orders.write().pop();
                    continue;
                }
            };

            // Determine match price (earlier order's price)
            let match_price = if buy_order.submit_time <= sell_order.submit_time {
                buy_order.price
            } else {
                sell_order.price
            };

            // Calculate match volume
            let buy_remaining = buy_order.remaining_volume();
            let sell_remaining = sell_order.remaining_volume();
            let match_volume = buy_remaining.min(sell_remaining);

            // Update orders
            buy_order.executed_volume += match_volume;
            sell_order.executed_volume += match_volume;
            buy_order.frozen_amount -= match_volume * match_price;
            sell_order.frozen_amount -= match_volume;

            // Update price
            *self.price.write() = match_price;

            // Record trade
            let trade = TradeEntry {
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                price: match_price,
                volume: match_volume,
            };
            self.trade_log.write().push(trade);

            // Check if orders are filled
            let buy_filled = buy_order.is_filled();
            let sell_filled = sell_order.is_filled();

            drop(buy_order);
            drop(sell_order);

            if buy_filled {
                if let Some(mut order) = self.orders.get_mut(&buy_priority.order_id) {
                    order.status = OrderStatus::Filled;
                }
                self.orders.remove(&buy_priority.order_id);
                self.buy_orders.write().pop();
            }

            if sell_filled {
                if let Some(mut order) = self.orders.get_mut(&sell_priority.order_id) {
                    order.status = OrderStatus::Filled;
                }
                self.orders.remove(&sell_priority.order_id);
                self.sell_orders.write().pop();
            }
        }
    }

    /// Cancel an order
    pub fn cancel_order(&self, order_id: u64) -> Option<Order> {
        if let Some((_, mut order)) = self.orders.remove(&order_id) {
            order.status = OrderStatus::Cancelled;
            Some(order)
        } else {
            None
        }
    }

    /// Get order book snapshot
    pub fn get_order_book(&self) -> (Vec<Order>, Vec<Order>) {
        let buy_orders: Vec<Order> = self
            .orders
            .iter()
            .filter(|o| o.direction == Direction::Buy)
            .map(|o| o.clone())
            .collect();

        let sell_orders: Vec<Order> = self
            .orders
            .iter()
            .filter(|o| o.direction == Direction::Sell)
            .map(|o| o.clone())
            .collect();

        (buy_orders, sell_orders)
    }
}
