use barter_data::exchange::aevo::Aevo;
use barter_data::streams::Streams;
use barter_data::subscription::book::OrderBook;
use barter_data::subscription::book::OrderBooksL2;
use barter_integration::model::instrument::kind::InstrumentKind;
use chrono::Utc;
use std::thread;
use std::time::Duration;
use tracing::info;

// Constants
const TRADE_SIZE: f64 = 0.001;
const SPREAD_THRESHOLD: f64 = 0.05; // Adjust based on backtesting and performance analysis
const TAKE_PROFIT: f64 = 0.01; // 1%
const STOP_LOSS: f64 = 0.02; // 2%
const TRANSACTION_COST: f64 = 0.005; // 0.5%

// Struct to hold the trading state
#[derive(Debug)]
struct TradingState {
    cash: f64,
    positions: Vec<f64>,
    symbol: &'static str,
}

impl TradingState {
    fn new(cash: f64, symbol: &'static str) -> Self {
        Self {
            cash,
            positions: Vec::new(),
            symbol,
        }
    }

    fn calculate_voi(order_book: &OrderBook) -> (f64, f64, f64) {
        let bid_volume: f64 = order_book.bids.levels.iter().map(|bid| bid.amount).sum();
        let ask_volume: f64 = order_book.asks.levels.iter().map(|ask| ask.amount).sum();
        let voi: f64 = bid_volume - ask_volume;
        (voi, bid_volume, ask_volume)
    }

    fn calculate_oir(bid_volume: f64, ask_volume: f64) -> f64 {
        (bid_volume - ask_volume) / (bid_volume + ask_volume)
    }

    fn calculate_mpb(last_price: f64, mid_price: f64) -> f64 {
        last_price - mid_price
    }

    fn calculate_spread(bid: f64, ask: f64) -> f64 {
        (ask - bid) / bid * 100.0
    }

    fn should_trade(spread: f64, voi: f64, spread_threshold: f64) -> bool {
        spread <= spread_threshold && voi.abs() > 0.0
    }

    fn execute_trade(&mut self, price: f64, side: &str, trade_size: f64, fee: f64) {
        let transaction_cost = trade_size * price * fee;
        if side == "buy" {
            self.positions.push(price);
            self.cash -= price * trade_size + transaction_cost;
            info!(
                "Buying {} {} at {} (cost: {}) at {}",
                trade_size,
                self.symbol,
                price,
                transaction_cost,
                Utc::now()
            );
        } else if side == "sell" {
            if let Some(_position) = self.positions.pop() {
                self.cash += price * trade_size - transaction_cost;
                info!(
                    "Selling {} {} at {} (cost: {}) at {}",
                    trade_size,
                    self.symbol,
                    price,
                    transaction_cost,
                    Utc::now()
                );
            }
        }
    }

    fn check_tp_sl(&mut self, bid: f64, tp: f64, sl: f64) {
        let mut positions_to_sell: Vec<f64> = Vec::new();

        for position in &self.positions {
            let profit_loss = (bid - *position) / *position;
            if profit_loss >= tp {
                info!(
                    "Triggering Take Profit: Selling position at {} with profit/loss: {:.2}%",
                    bid,
                    profit_loss * 100.0
                );
                positions_to_sell.push(*position);
            } else if profit_loss <= -sl {
                info!(
                    "Triggering Stop Loss: Selling position at {} with profit/loss: {:.2}%",
                    bid,
                    profit_loss * 100.0
                );
                positions_to_sell.push(*position);
            }
        }

        for position in positions_to_sell {
            self.positions.retain(|&x| x != position);
            self.execute_trade(bid, "sell", TRADE_SIZE, TRANSACTION_COST);
        }
    }

    fn calculate_portfolio_value(&self, bid: f64) -> f64 {
        let position_value: f64 = self.positions.len() as f64 * TRADE_SIZE * bid;
        self.cash + position_value
    }
}

#[tokio::main]
async fn main() {
    init_logging();

    let mut trading_state = TradingState::new(1000.0, "BTC/USDT");

    // TODO: Add order book streams from other exchanges, then merge them
    let streams = Streams::<OrderBooksL2>::builder()
        .subscribe([(Aevo, "btc", "usd", InstrumentKind::Perpetual, OrderBooksL2)])
        .init()
        .await
        .unwrap();

    let mut joined_stream = streams.join().await;

    while let Some(market_event) = joined_stream.recv().await {
        let order_book = market_event.kind;
        let bid: f64 = order_book.bids.levels[0].price;
        let ask: f64 = order_book.asks.levels[0].price;
        let spread: f64 = TradingState::calculate_spread(bid, ask);
        let last_price: f64 = (bid + ask) / 2.0;

        // Calculate volume order imbalance
        let (voi, bid_volume, ask_volume) = TradingState::calculate_voi(&order_book);

        // Calculate Order Imbalance Ratio (OIR)
        let oir: f64 = TradingState::calculate_oir(bid_volume, ask_volume);

        // Calculate Mid-Price Basis (MPB)
        let mpb: f64 = TradingState::calculate_mpb(last_price, (bid + ask) / 2.0);

        // Check if a trade should be made
        if TradingState::should_trade(spread, voi, SPREAD_THRESHOLD) {
            // Buy at the bid price if VOI is positive and OIR indicates a strong buy signal
            if voi > 0.0 && oir > 0.1 {
                trading_state.execute_trade(bid, "buy", TRADE_SIZE, TRANSACTION_COST);
            }
            // Sell at the ask price if VOI is negative and MPB indicates a strong sell signal
            else if voi < 0.0 && mpb < -0.1 && !trading_state.positions.is_empty() {
                trading_state.execute_trade(ask, "sell", TRADE_SIZE, TRANSACTION_COST);
            }
        }

        // Check for Take Profit or Stop Loss conditions
        trading_state.check_tp_sl(bid, TAKE_PROFIT, STOP_LOSS);

        // Calculate the current portfolio value
        let portfolio_value = trading_state.calculate_portfolio_value(bid);
        info!(
            "Current portfolio value: ${:.2} at {}",
            portfolio_value,
            Utc::now()
        );

        // Sleep before the next iteration
        thread::sleep(Duration::from_secs(1));
    }
}

// Initialise an INFO `Subscriber` for `Tracing` Json logs and install it as the global default.
fn init_logging() {
    tracing_subscriber::fmt()
        // Filter messages based on the INFO
        .with_env_filter(
            tracing_subscriber::filter::EnvFilter::builder()
                .with_default_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        // Disable colours on release builds
        .with_ansi(cfg!(debug_assertions))
        // Enable Json formatting
        .pretty()
        // Install this Tracing subscriber as global default
        .init()
}

#[cfg(test)]
mod tests {
    use barter_data::subscription::book::Level;
    use barter_data::subscription::book::OrderBookSide;
    use barter_integration::model::Side;
    use chrono::DateTime;

    use super::*;

    // Constants for tests
    const TEST_TRADE_SIZE: f64 = 0.001;
    const TEST_TAKE_PROFIT: f64 = 0.01;
    const TEST_STOP_LOSS: f64 = 0.02;
    const TEST_TRANSACTION_COST: f64 = 0.005;
    const TEST_SPREAD_THRESHOLD: f64 = 0.05;
    const FLOAT_TOLERANCE: f64 = 0.00001;

    /// Helper function that rounds the floating-point numbers to a specified number of decimal places
    /// before comparing them.
    fn approx_equal(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() < tolerance
    }

    #[test]
    fn test_calculate_voi() {
        let order_book = OrderBook {
            last_update_time: DateTime::from_timestamp_millis(0).unwrap(),
            bids: OrderBookSide::new(
                Side::Buy,
                vec![Level {
                    price: 100.0,
                    amount: 1.0,
                }],
            ),
            asks: OrderBookSide::new(
                Side::Sell,
                vec![Level {
                    price: 101.0,
                    amount: 1.0,
                }],
            ),
        };

        let (voi, bid_volume, ask_volume) = TradingState::calculate_voi(&order_book);
        assert_eq!(voi, 0.0);
        assert_eq!(bid_volume, 1.0);
        assert_eq!(ask_volume, 1.0);
    }

    #[test]
    fn test_calculate_oir() {
        let oir = TradingState::calculate_oir(1.0, 1.0);
        assert_eq!(oir, 0.0);
    }

    #[test]
    fn test_calculate_mpb() {
        let mpb = TradingState::calculate_mpb(100.0, 100.0);
        assert_eq!(mpb, 0.0);
    }

    #[test]
    fn test_calculate_spread() {
        let spread = TradingState::calculate_spread(100.0, 101.0);
        assert_eq!(spread, 1.0);
    }

    #[test]
    fn test_should_trade() {
        let valid_voi = 1.0;
        assert!(TradingState::should_trade(
            TEST_SPREAD_THRESHOLD,
            valid_voi,
            TEST_SPREAD_THRESHOLD
        ));

        let invalid_spread = 0.06;
        let valid_voi = 1.0;
        assert!(!TradingState::should_trade(
            invalid_spread,
            valid_voi,
            TEST_SPREAD_THRESHOLD
        ));

        let invalid_voi = 0.0;
        assert!(!TradingState::should_trade(
            TEST_SPREAD_THRESHOLD,
            invalid_voi,
            TEST_SPREAD_THRESHOLD
        ));
    }

    #[test]
    fn test_execute_trade() {
        let mut state = TradingState::new(1000.0, "BTC/USDT");
        state.execute_trade(100.0, "buy", TEST_TRADE_SIZE, TEST_TRANSACTION_COST);
        let expected_cash_after_buy =
            1000.0 - (100.0 * TEST_TRADE_SIZE) - (100.0 * TEST_TRADE_SIZE * TEST_TRANSACTION_COST);
        assert_eq!(state.cash, expected_cash_after_buy);
        assert_eq!(state.positions.len(), 1);

        state.execute_trade(100.0, "sell", TEST_TRADE_SIZE, TEST_TRANSACTION_COST);
        let expected_cash_after_sell = expected_cash_after_buy + (100.0 * TEST_TRADE_SIZE)
            - (100.0 * TEST_TRADE_SIZE * TEST_TRANSACTION_COST);
        assert_eq!(state.cash, expected_cash_after_sell);
        assert_eq!(state.positions.len(), 0);
    }

    #[test]
    fn test_check_tp_sl() {
        let mut state = TradingState::new(1000.0, "BTC/USDT");

        // Testing Take Profit
        state.positions.push(100.0);
        state.check_tp_sl(102.0, TEST_TAKE_PROFIT, TEST_STOP_LOSS);
        let profit = 100.0 * TEST_TRADE_SIZE * (1.0 + TEST_TAKE_PROFIT);
        let transaction_cost_tp = 102.0 * TEST_TRADE_SIZE * TEST_TRANSACTION_COST;
        let expected_cash_after_tp = 1000.0 + profit - transaction_cost_tp;
        assert_eq!(state.positions.len(), 0);
        assert!(
            approx_equal(state.cash, expected_cash_after_tp, FLOAT_TOLERANCE),
            "Cash after TP not as expected. Got: {}, Expected: {}",
            state.cash,
            expected_cash_after_tp
        );

        // Testing Stop Loss
        state.positions.push(100.0);
        state.check_tp_sl(98.0, TEST_TAKE_PROFIT, TEST_STOP_LOSS);
        let loss = 100.0 * TEST_TRADE_SIZE * (1.0 - TEST_STOP_LOSS);
        let transaction_cost_sl = 98.0 * TEST_TRADE_SIZE * TEST_TRANSACTION_COST;
        let expected_cash_after_sl = expected_cash_after_tp - loss - transaction_cost_sl;
        assert_eq!(state.positions.len(), 0);
        assert!(
            approx_equal(state.cash, expected_cash_after_sl, FLOAT_TOLERANCE),
            "Cash after SL not as expected. Got: {}, Expected: {}",
            state.cash,
            expected_cash_after_sl
        );
    }

    #[test]
    fn test_calculate_portfolio_value() {
        let mut state = TradingState::new(1000.0, "BTC/USDT");
        state.positions.push(100.0);
        let portfolio_value = state.calculate_portfolio_value(101.0);
        let expected_portfolio_value = 1000.0 + (101.0 * TEST_TRADE_SIZE);
        assert_eq!(portfolio_value, expected_portfolio_value);
    }
}
