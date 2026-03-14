use crate::bot::BotManager;
use crate::finance::{Token, TradingPair};
use crate::protocol::{
    parse_decimal, parse_direction, OrderSerializable, Request, Response, TradeEntrySerializable,
    TradingPairInfo, TraderInfoSerializable,
};
use crate::trader::Trader;
use dashmap::DashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::interval;

/// Server state
pub struct Server {
    pub tokens: DashMap<String, Token>,
    pub trading_pairs: DashMap<String, Arc<TradingPair>>,
    pub traders: DashMap<u64, Arc<Trader>>,
    pub bot_manager: RwLock<Option<Arc<BotManager>>>,
    pub simulation_running: AtomicBool,
    pub simulation_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            tokens: DashMap::new(),
            trading_pairs: DashMap::new(),
            traders: DashMap::new(),
            bot_manager: RwLock::new(None),
            simulation_running: AtomicBool::new(false),
            simulation_handle: RwLock::new(None),
        }
    }

    pub fn create_token(&self, name: String) -> Response {
        let token = Token::new(&name);
        self.tokens.insert(name.clone(), token);
        Response::TokenCreated { name }
    }

    pub fn create_trading_pair(
        &self,
        base_token: String,
        quote_token: String,
        initial_price: String,
    ) -> Response {
        let price = match parse_decimal(&initial_price) {
            Some(p) => p,
            None => {
                return Response::Error {
                    message: "Invalid price format".to_string(),
                }
            }
        };

        let base = match self.tokens.get(&base_token) {
            Some(t) => t.clone(),
            None => {
                return Response::Error {
                    message: format!("Base token {} not found", base_token),
                }
            }
        };

        let quote = match self.tokens.get(&quote_token) {
            Some(t) => t.clone(),
            None => {
                return Response::Error {
                    message: format!("Quote token {} not found", quote_token),
                }
            }
        };

        let pair = TradingPair::new(base, quote, price);
        let id = pair.id.clone();
        let current_price = pair.price.read().to_string();

        self.trading_pairs.insert(id.clone(), pair);

        Response::TradingPairCreated {
            id,
            base_token,
            quote_token,
            price: current_price,
        }
    }

    pub fn create_bots(
        &self,
        count: usize,
        asset_configs: HashMap<String, (String, String)>,
        name_prefix: String,
        trend: f64,
        view: f64,
    ) -> Response {
        let bot_manager = BotManager::new();

        // Convert asset configs
        let mut configs: HashMap<String, (Decimal, Decimal)> = HashMap::new();
        for (token, (min_str, max_str)) in asset_configs {
            let min = match parse_decimal(&min_str) {
                Some(m) => m,
                None => continue,
            };
            let max = match parse_decimal(&max_str) {
                Some(m) => m,
                None => continue,
            };
            configs.insert(token, (min, max));
        }

        let bots = bot_manager.create_bots_batch(count, &configs, &name_prefix, trend, view);

        // Set trading pairs for bots
        let pairs: Vec<Arc<TradingPair>> = self
            .trading_pairs
            .iter()
            .map(|e| Arc::clone(e.value()))
            .collect();
        bot_manager.set_trading_pairs(pairs);

        let bot_ids: Vec<u64> = bots.iter().map(|b| b.id).collect();

        *self.bot_manager.write() = Some(bot_manager);

        Response::BotsCreated {
            count: bot_ids.len(),
            bot_ids,
        }
    }

    pub fn start_simulation(&self) -> Response {
        if self.simulation_running.load(Ordering::SeqCst) {
            return Response::Error {
                message: "Simulation already running".to_string(),
            };
        }

        let bot_manager = match self.bot_manager.read().as_ref() {
            Some(bm) => Arc::clone(bm),
            None => {
                return Response::Error {
                    message: "No bots created".to_string(),
                }
            }
        };

        self.simulation_running.store(true, Ordering::SeqCst);

        let handle = tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100));

            while bot_manager.len() > 0 {
                bot_manager.step();
                interval.tick().await;
            }
        });

        *self.simulation_handle.write() = Some(handle);

        Response::Success {
            message: "Simulation started".to_string(),
        }
    }

    pub fn stop_simulation(&self) -> Response {
        self.simulation_running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.simulation_handle.write().take() {
            handle.abort();
        }

        Response::Success {
            message: "Simulation stopped".to_string(),
        }
    }

    pub fn submit_limit_order(
        &self,
        trader_id: u64,
        trading_pair_id: String,
        direction: String,
        price: String,
        volume: String,
    ) -> Response {
        let trader = match self.traders.get(&trader_id) {
            Some(t) => Arc::clone(t.value()),
            None => {
                return Response::Error {
                    message: "Trader not found".to_string(),
                }
            }
        };

        let pair = match self.trading_pairs.get(&trading_pair_id) {
            Some(p) => Arc::clone(p.value()),
            None => {
                return Response::Error {
                    message: "Trading pair not found".to_string(),
                }
            }
        };

        let dir = match parse_direction(&direction) {
            Some(d) => d,
            None => {
                return Response::Error {
                    message: "Invalid direction".to_string(),
                }
            }
        };

        let price_dec = match parse_decimal(&price) {
            Some(p) => p,
            None => {
                return Response::Error {
                    message: "Invalid price".to_string(),
                }
            }
        };

        let volume_dec = match parse_decimal(&volume) {
            Some(v) => v,
            None => {
                return Response::Error {
                    message: "Invalid volume".to_string(),
                }
            }
        };

        match trader.submit(&pair, dir, price_dec, volume_dec) {
            Some(order) => Response::OrderSubmitted { order_id: order.id },
            None => Response::Error {
                message: "Failed to submit order".to_string(),
            },
        }
    }

    pub fn submit_market_order(
        &self,
        trader_id: u64,
        trading_pair_id: String,
        direction: String,
        volume: String,
    ) -> Response {
        let trader = match self.traders.get(&trader_id) {
            Some(t) => Arc::clone(t.value()),
            None => {
                return Response::Error {
                    message: "Trader not found".to_string(),
                }
            }
        };

        let pair = match self.trading_pairs.get(&trading_pair_id) {
            Some(p) => Arc::clone(p.value()),
            None => {
                return Response::Error {
                    message: "Trading pair not found".to_string(),
                }
            }
        };

        let dir = match parse_direction(&direction) {
            Some(d) => d,
            None => {
                return Response::Error {
                    message: "Invalid direction".to_string(),
                }
            }
        };

        let volume_dec = match parse_decimal(&volume) {
            Some(v) => v,
            None => {
                return Response::Error {
                    message: "Invalid volume".to_string(),
                }
            }
        };

        match trader.submit_market(&pair, dir, volume_dec) {
            Some(trades) => {
                let trades: Vec<TradeEntrySerializable> =
                    trades.iter().map(|t| t.into()).collect();
                Response::MarketOrderExecuted { trades }
            }
            None => Response::Error {
                message: "Failed to execute market order".to_string(),
            },
        }
    }

    pub fn cancel_order(
        &self,
        trader_id: u64,
        order_id: u64,
        trading_pair_id: String,
    ) -> Response {
        let trader = match self.traders.get(&trader_id) {
            Some(t) => Arc::clone(t.value()),
            None => {
                return Response::Error {
                    message: "Trader not found".to_string(),
                }
            }
        };

        let pair = match self.trading_pairs.get(&trading_pair_id) {
            Some(p) => Arc::clone(p.value()),
            None => {
                return Response::Error {
                    message: "Trading pair not found".to_string(),
                }
            }
        };

        match trader.cancel_order(order_id, &pair) {
            Some(_) => Response::OrderCancelled { order_id },
            None => Response::Error {
                message: "Order not found".to_string(),
            },
        }
    }

    pub fn get_trade_log(&self, trading_pair_id: String, limit: Option<usize>) -> Response {
        let pair = match self.trading_pairs.get(&trading_pair_id) {
            Some(p) => p,
            None => {
                return Response::Error {
                    message: "Trading pair not found".to_string(),
                }
            }
        };

        let log = pair.trade_log.read();
        let trades: Vec<TradeEntrySerializable> = log
            .iter()
            .rev()
            .take(limit.unwrap_or(usize::MAX))
            .map(|t| t.into())
            .collect();

        Response::TradeLog {
            trading_pair_id,
            trades,
        }
    }

    pub fn get_order_book(&self, trading_pair_id: String) -> Response {
        let pair = match self.trading_pairs.get(&trading_pair_id) {
            Some(p) => p,
            None => {
                return Response::Error {
                    message: "Trading pair not found".to_string(),
                }
            }
        };

        let (buy_orders, sell_orders) = pair.get_order_book();

        Response::OrderBook {
            trading_pair_id,
            buy_orders: buy_orders.iter().map(|o| o.into()).collect(),
            sell_orders: sell_orders.iter().map(|o| o.into()).collect(),
        }
    }

    pub fn get_trader_info(&self, trader_id: u64) -> Response {
        let trader = match self.traders.get(&trader_id) {
            Some(t) => t,
            None => {
                return Response::Error {
                    message: "Trader not found".to_string(),
                }
            }
        };

        let info = trader.get_info();
        Response::TraderInfo {
            info: (&info).into(),
        }
    }

    pub fn get_all_trading_pairs(&self) -> Response {
        let pairs: Vec<TradingPairInfo> = self
            .trading_pairs
            .iter()
            .map(|e| {
                let pair = e.value();
                TradingPairInfo {
                    id: pair.id.clone(),
                    base_token: pair.base_token.name.clone(),
                    quote_token: pair.quote_token.name.clone(),
                    price: pair.price.read().to_string(),
                }
            })
            .collect();

        Response::TradingPairsList { pairs }
    }

    pub fn get_market_data(&self, trading_pair_id: String) -> Response {
        let pair = match self.trading_pairs.get(&trading_pair_id) {
            Some(p) => p,
            None => {
                return Response::Error {
                    message: "Trading pair not found".to_string(),
                }
            }
        };

        let current_price = pair.price.read().to_string();
        let trade_count = pair.trade_log.read().len();

        let (buy_orders, sell_orders) = pair.get_order_book();

        Response::MarketData {
            trading_pair_id,
            current_price,
            trade_count,
            buy_order_count: buy_orders.len(),
            sell_order_count: sell_orders.len(),
        }
    }

    pub fn create_player(&self, name: String, assets: HashMap<String, String>) -> Response {
        let trader = Trader::new(&name);

        for (token, amount_str) in assets {
            if let Some(amount) = parse_decimal(&amount_str) {
                trader.add_asset(token, amount);
            }
        }

        let trader_id = trader.id;
        self.traders.insert(trader_id, trader);

        Response::PlayerCreated { trader_id }
    }

    pub fn get_simulation_status(&self) -> Response {
        let running = self.simulation_running.load(Ordering::SeqCst);
        let bot_count = self
            .bot_manager
            .read()
            .as_ref()
            .map(|bm| bm.len())
            .unwrap_or(0);

        Response::SimulationStatus { running, bot_count }
    }
}

/// Handle a single request
pub fn handle_request(server: &Server, request: Request) -> Response {

    match request {
        Request::CreateToken { name } => server.create_token(name),
        Request::CreateTradingPair {
            base_token,
            quote_token,
            initial_price,
        } => server.create_trading_pair(base_token, quote_token, initial_price),
        Request::CreateBots {
            count,
            asset_configs,
            name_prefix,
            trend,
            view,
        } => server.create_bots(count, asset_configs, name_prefix, trend, view),
        Request::StartSimulation => server.start_simulation(),
        Request::StopSimulation => server.stop_simulation(),
        Request::SubmitLimitOrder {
            trader_id,
            trading_pair_id,
            direction,
            price,
            volume,
        } => server.submit_limit_order(trader_id, trading_pair_id, direction, price, volume),
        Request::SubmitMarketOrder {
            trader_id,
            trading_pair_id,
            direction,
            volume,
        } => server.submit_market_order(trader_id, trading_pair_id, direction, volume),
        Request::CancelOrder {
            trader_id,
            order_id,
            trading_pair_id,
        } => server.cancel_order(trader_id, order_id, trading_pair_id),
        Request::GetTradeLog {
            trading_pair_id,
            limit,
        } => server.get_trade_log(trading_pair_id, limit),
        Request::GetOrderBook { trading_pair_id } => server.get_order_book(trading_pair_id),
        Request::GetTraderInfo { trader_id } => server.get_trader_info(trader_id),
        Request::GetAllTradingPairs => server.get_all_trading_pairs(),
        Request::GetMarketData { trading_pair_id } => server.get_market_data(trading_pair_id),
        Request::CreatePlayer { name, assets } => server.create_player(name, assets),
    }
}

/// Run the TCP server
pub async fn run_server(
    server: Arc<RwLock<Server>>,
    addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr).await?;
    println!("Server listening on {}", addr);

    loop {
        let (socket, _) = listener.accept().await?;
        let server_clone = Arc::clone(&server);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, server_clone).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
}

/// Handle a single connection
async fn handle_connection(
    socket: TcpStream,
    server: Arc<RwLock<Server>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            // Connection closed
            break;
        }

        // Parse request
        let request: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let response = Response::Error {
                    message: format!("Invalid request: {}", e),
                };
                let response_json = serde_json::to_string(&response)?;
                writer.write_all(response_json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                continue;
            }
        };

        // Handle request - acquire lock for each request
        let response = {
            let server_guard = server.read();
            handle_request(&*server_guard, request)
        };

        // Send response
        let response_json = serde_json::to_string(&response)?;
        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
    }

    Ok(())
}
