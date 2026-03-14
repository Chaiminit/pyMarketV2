use crate::finance::{Direction, TradingPair};
use crate::trader::Trader;
use crate::utils::{decimal_to_f64, sigmoid, ChipDistribution, f64_to_decimal};
use parking_lot::RwLock;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static BOT_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_bot_id() -> u64 {
    BOT_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// RandomBot implements random trading strategy
pub struct RandomBot {
    pub id: u64,
    pub trader: Arc<Trader>,
    pub trend: f64,
    pub view: f64,
    pub k: RwLock<f64>,
    pub trading_pairs: RwLock<Vec<Arc<TradingPair>>>,
    pub chip_dist: ChipDistribution,
    pub rng: RwLock<StdRng>,
}

impl RandomBot {
    pub fn new(name: impl Into<String>, trend: f64, view: f64) -> Arc<Self> {
        let trader = Trader::new(name);
        let seed = next_bot_id();

        Arc::new(Self {
            id: seed,
            trader,
            trend,
            view,
            k: RwLock::new(0.0),
            trading_pairs: RwLock::new(Vec::new()),
            chip_dist: ChipDistribution::new(0.05, 1.0, Some(0.6)),
            rng: RwLock::new(StdRng::seed_from_u64(seed)),
        })
    }

    pub fn set_trading_pairs(&self, pairs: Vec<Arc<TradingPair>>) {
        *self.trading_pairs.write() = pairs;
    }

    pub fn add_asset(&self, token_name: impl Into<String>, amount: Decimal) {
        self.trader.add_asset(token_name, amount);
    }

    pub fn act(&self) {
        let pairs = self.trading_pairs.read().clone();

        if pairs.is_empty() {
            return;
        }

        // Limit number of orders
        {
            let orders = self.trader.orders.read();
            let max_orders = 10 * pairs.len();
            if orders.len() > max_orders {
                // Cancel oldest order (simplified)
                // In real implementation, we'd need to track which trading pair
            }
        }

        for trading_pair in &pairs {
            let mut rng = self.rng.write();
            let action: f64 = rng.gen();
            drop(rng);

            let trade_log = trading_pair.trade_log.read();

            let mut final_action = action;

            if !trade_log.is_empty() {
                let current_time = trade_log.last().unwrap().timestamp;
                let target_time = current_time as f64 - self.view;

                // Find start index using binary search
                let start_index = Self::find_start_index(&trade_log, target_time as u64);

                let current_price = decimal_to_f64(trade_log.last().unwrap().price);
                let start_price = decimal_to_f64(trade_log[start_index].price);

                let price_change =
                    (current_price - start_price) / start_price / self.view * self.trend;
                let new_k = sigmoid((sigmoid(price_change) - 0.5) * 10.0) - 0.5;

                let mut k = self.k.write();
                let sh = 0.5 + self.rng.write().gen::<f64>() * 0.5;
                final_action = action * 0.6 + ((*k - new_k) * sh + new_k * (1.0 - sh));

                let ra = (self.rng.write().gen::<f64>() + 3.0) / 4.0;
                *k = *k * ra + new_k * (1.0 - ra);
                drop(k);
            }

            drop(trade_log);

            // Execute action
            let result = if final_action < 0.1 {
                self.place_market_order(trading_pair, Direction::Sell)
            } else if final_action < 0.5 {
                self.place_limit_order(trading_pair, Direction::Sell)
            } else if final_action < 0.9 {
                self.place_limit_order(trading_pair, Direction::Buy)
            } else {
                self.place_market_order(trading_pair, Direction::Buy)
            };

            if let Err(e) = result {
                // Silently fail for now
            }
        }
    }

    fn find_start_index(trade_log: &[crate::finance::TradeEntry], target_time: u64) -> usize {
        let mut left = 0;
        let mut right = trade_log.len() - 1;
        let mut start_index = 0;

        while left <= right {
            let mid = (left + right) / 2;
            if trade_log[mid].timestamp >= target_time {
                start_index = mid;
                if mid == 0 {
                    break;
                }
                right = mid - 1;
            } else {
                left = mid + 1;
            }
        }

        start_index.min(trade_log.len() - 1)
    }

    fn place_limit_order(
        &self,
        trading_pair: &Arc<TradingPair>,
        direction: Direction,
    ) -> Result<(), String> {
        let mut rng = self.rng.write();
        let d: f64 = rng.gen();
        drop(rng);

        let current_price = *trading_pair.price.read();
        let current_price_f64 = decimal_to_f64(current_price);

        let price = match direction {
            Direction::Buy => {
                let price_f64 = current_price_f64 * (1.0 + d * (-d / 0.05).exp());
                f64_to_decimal(price_f64)
            }
            Direction::Sell => {
                let price_f64 = current_price_f64 * (1.0 - d * (-d / 0.05).exp());
                f64_to_decimal(price_f64)
            }
        };

        let max_volume = match direction {
            Direction::Buy => {
                let quote_amount =
                    self.trader
                        .get_asset(&trading_pair.quote_token.name);
                if quote_amount <= Decimal::ZERO {
                    return Err("Insufficient quote token".to_string());
                }
                let price_diff = (price - current_price).abs() / current_price;
                let price_diff_f64 = decimal_to_f64(price_diff);
                let pdf = self.chip_dist.pdf(price_diff_f64);
                let mut rng = self.rng.write();
                let factor = (rng.gen::<f64>() + 3.0) / 4.0;
                drop(rng);

                quote_amount * f64_to_decimal(pdf * factor) / price
            }
            Direction::Sell => {
                let base_amount =
                    self.trader
                        .get_asset(&trading_pair.base_token.name);
                if base_amount <= Decimal::ZERO {
                    return Err("Insufficient base token".to_string());
                }
                let price_diff = (price - current_price).abs() / current_price;
                let price_diff_f64 = decimal_to_f64(price_diff);
                let pdf = self.chip_dist.pdf(price_diff_f64);
                let mut rng = self.rng.write();
                let factor = (rng.gen::<f64>() + 3.0) / 4.0;
                drop(rng);

                base_amount * f64_to_decimal(pdf * factor)
            }
        };

        let volume = max_volume.max(f64_to_decimal(0.0001));

        self.trader
            .submit(trading_pair, direction, price, volume)
            .ok_or("Failed to submit order".to_string())?;

        Ok(())
    }

    fn place_market_order(
        &self,
        trading_pair: &Arc<TradingPair>,
        direction: Direction,
    ) -> Result<(), String> {
        let mut rng = self.rng.write();
        let random_factor: f64 = rng.gen();
        drop(rng);

        let volume = match direction {
            Direction::Buy => {
                let quote_amount =
                    self.trader
                        .get_asset(&trading_pair.quote_token.name);
                if quote_amount <= Decimal::ZERO {
                    return Err("Insufficient quote token".to_string());
                }
                let current_price = *trading_pair.price.read();
                let volume_f64 =
                    decimal_to_f64(quote_amount) * (1.0 - random_factor.powf(3.5))
                        / decimal_to_f64(current_price);
                f64_to_decimal(volume_f64.max(0.000001))
            }
            Direction::Sell => {
                let base_amount =
                    self.trader
                        .get_asset(&trading_pair.base_token.name);
                if base_amount <= Decimal::ZERO {
                    return Err("Insufficient base token".to_string());
                }
                let volume_f64 =
                    decimal_to_f64(base_amount) * (1.0 - random_factor.powf(3.5));
                f64_to_decimal(volume_f64.max(0.000001))
            }
        };

        self.trader
            .submit_market(trading_pair, direction, volume)
            .ok_or("Failed to submit market order".to_string())?;

        Ok(())
    }

    pub fn get_asset_value(&self, token_name: &str) -> Decimal {
        self.trader.get_asset(token_name)
    }
}

/// BotManager manages multiple bots
pub struct BotManager {
    pub bots: RwLock<Vec<Arc<RandomBot>>>,
}

impl BotManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            bots: RwLock::new(Vec::new()),
        })
    }

    pub fn create_bots_batch(
        &self,
        count: usize,
        asset_configs: &HashMap<String, (Decimal, Decimal)>,
        name_prefix: &str,
        trend: f64,
        view: f64,
    ) -> Vec<Arc<RandomBot>> {
        let mut bots = Vec::with_capacity(count);
        let mut rng = StdRng::from_entropy();

        for i in 0..count {
            let bot_trend = trend * (-0.7 + rng.gen::<f64>() * 1.5);
            let bot_view = view * (0.1 + rng.gen::<f64>() * 1.9);

            let bot = RandomBot::new(format!("{}_{:03}", name_prefix, i + 1), bot_trend, bot_view);

            // Allocate assets
            for (token_name, (min_amount, max_amount)) in asset_configs {
                let range = *max_amount - *min_amount;
                let amount = *min_amount + f64_to_decimal(rng.gen::<f64>()) * range;
                bot.add_asset(token_name.clone(), amount);
            }

            bots.push(bot);
        }

        let bot_refs: Vec<Arc<RandomBot>> = bots.iter().map(|b| Arc::clone(b)).collect();
        *self.bots.write() = bots;
        bot_refs
    }

    pub fn set_trading_pairs(&self, pairs: Vec<Arc<TradingPair>>) {
        for bot in self.bots.read().iter() {
            bot.set_trading_pairs(pairs.clone());
        }
    }

    pub fn step(&self) {
        for bot in self.bots.read().iter() {
            let mut rng = StdRng::from_entropy();
            if rng.gen::<f64>() < 0.7 {
                bot.act();
            }
        }
    }

    pub fn len(&self) -> usize {
        self.bots.read().len()
    }
}
