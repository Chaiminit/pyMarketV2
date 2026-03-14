use crate::finance::{Direction, Order, Token, TradeEntry, TradingPair};
use crate::trader::TraderInfo;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Request types from Python frontend to Rust backend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    // Market management
    #[serde(rename = "create_token")]
    CreateToken { name: String },

    #[serde(rename = "create_trading_pair")]
    CreateTradingPair {
        base_token: String,
        quote_token: String,
        initial_price: String, // Decimal as string
    },

    // Bot management
    #[serde(rename = "create_bots")]
    CreateBots {
        count: usize,
        asset_configs: HashMap<String, (String, String)>, // token -> (min, max) as strings
        name_prefix: String,
        trend: f64,
        view: f64,
    },

    #[serde(rename = "start_simulation")]
    StartSimulation,

    #[serde(rename = "stop_simulation")]
    StopSimulation,

    // Trading
    #[serde(rename = "submit_limit_order")]
    SubmitLimitOrder {
        trader_id: u64,
        trading_pair_id: String,
        direction: String, // "buy" or "sell"
        price: String,     // Decimal as string
        volume: String,    // Decimal as string
    },

    #[serde(rename = "submit_market_order")]
    SubmitMarketOrder {
        trader_id: u64,
        trading_pair_id: String,
        direction: String, // "buy" or "sell"
        volume: String,    // Decimal as string
    },

    #[serde(rename = "cancel_order")]
    CancelOrder {
        trader_id: u64,
        order_id: u64,
        trading_pair_id: String,
    },

    // Data queries
    #[serde(rename = "get_trade_log")]
    GetTradeLog {
        trading_pair_id: String,
        limit: Option<usize>,
    },

    #[serde(rename = "get_order_book")]
    GetOrderBook { trading_pair_id: String },

    #[serde(rename = "get_trader_info")]
    GetTraderInfo { trader_id: u64 },

    #[serde(rename = "get_all_trading_pairs")]
    GetAllTradingPairs,

    #[serde(rename = "get_market_data")]
    GetMarketData {
        trading_pair_id: String,
    },

    // Player management
    #[serde(rename = "create_player")]
    CreatePlayer {
        name: String,
        assets: HashMap<String, String>, // token -> amount as string
    },
}

/// Response types from Rust backend to Python frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "success")]
    Success { message: String },

    #[serde(rename = "error")]
    Error { message: String },

    #[serde(rename = "token_created")]
    TokenCreated { name: String },

    #[serde(rename = "trading_pair_created")]
    TradingPairCreated {
        id: String,
        base_token: String,
        quote_token: String,
        price: String,
    },

    #[serde(rename = "bots_created")]
    BotsCreated { count: usize, bot_ids: Vec<u64> },

    #[serde(rename = "order_submitted")]
    OrderSubmitted { order_id: u64 },

    #[serde(rename = "market_order_executed")]
    MarketOrderExecuted { trades: Vec<TradeEntrySerializable> },

    #[serde(rename = "order_cancelled")]
    OrderCancelled { order_id: u64 },

    #[serde(rename = "trade_log")]
    TradeLog {
        trading_pair_id: String,
        trades: Vec<TradeEntrySerializable>,
    },

    #[serde(rename = "order_book")]
    OrderBook {
        trading_pair_id: String,
        buy_orders: Vec<OrderSerializable>,
        sell_orders: Vec<OrderSerializable>,
    },

    #[serde(rename = "trader_info")]
    TraderInfo { info: TraderInfoSerializable },

    #[serde(rename = "trading_pairs_list")]
    TradingPairsList { pairs: Vec<TradingPairInfo> },

    #[serde(rename = "market_data")]
    MarketData {
        trading_pair_id: String,
        current_price: String,
        trade_count: usize,
        buy_order_count: usize,
        sell_order_count: usize,
    },

    #[serde(rename = "player_created")]
    PlayerCreated { trader_id: u64 },

    #[serde(rename = "simulation_status")]
    SimulationStatus { running: bool, bot_count: usize },
}

/// Serializable trade entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeEntrySerializable {
    pub timestamp: u64,
    pub price: String,
    pub volume: String,
}

impl From<&TradeEntry> for TradeEntrySerializable {
    fn from(entry: &TradeEntry) -> Self {
        Self {
            timestamp: entry.timestamp,
            price: entry.price.to_string(),
            volume: entry.volume.to_string(),
        }
    }
}

/// Serializable order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSerializable {
    pub id: u64,
    pub submitter_id: u64,
    pub trading_pair_id: String,
    pub direction: String,
    pub price: String,
    pub total_volume: String,
    pub executed_volume: String,
    pub status: String,
}

impl From<&Order> for OrderSerializable {
    fn from(order: &Order) -> Self {
        Self {
            id: order.id,
            submitter_id: order.submitter_id,
            trading_pair_id: order.trading_pair_id.clone(),
            direction: order.direction.as_str().to_string(),
            price: order.price.to_string(),
            total_volume: order.total_volume.to_string(),
            executed_volume: order.executed_volume.to_string(),
            status: match order.status {
                crate::finance::OrderStatus::Active => "active".to_string(),
                crate::finance::OrderStatus::Filled => "filled".to_string(),
                crate::finance::OrderStatus::Cancelled => "cancelled".to_string(),
            },
        }
    }
}

/// Serializable trader info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraderInfoSerializable {
    pub id: u64,
    pub name: String,
    pub assets: HashMap<String, String>,
    pub orders: Vec<u64>,
}

impl From<&TraderInfo> for TraderInfoSerializable {
    fn from(info: &TraderInfo) -> Self {
        Self {
            id: info.id,
            name: info.name.clone(),
            assets: info
                .assets
                .iter()
                .map(|(k, v)| (k.clone(), v.to_string()))
                .collect(),
            orders: info.orders.clone(),
        }
    }
}

/// Trading pair info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPairInfo {
    pub id: String,
    pub base_token: String,
    pub quote_token: String,
    pub price: String,
}

/// Parse direction string
pub fn parse_direction(s: &str) -> Option<Direction> {
    match s.to_lowercase().as_str() {
        "buy" => Some(Direction::Buy),
        "sell" => Some(Direction::Sell),
        _ => None,
    }
}

/// Parse decimal from string
pub fn parse_decimal(s: &str) -> Option<Decimal> {
    s.parse().ok()
}
