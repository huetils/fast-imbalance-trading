use barter_data::exchange::aevo::Aevo;
use barter_data::streams::Streams;
use barter_data::subscription::book::OrderBook;
use barter_data::subscription::book::OrderBooksL2;
use barter_integration::model::instrument::kind::InstrumentKind;
use chrono::Utc;
use std::thread;
use std::time::Duration;
use tracing::info;

// Example trade size in BTC
const TRADE_SIZE: f64 = 0.01;

// Spread threshold for making a trade in percentage
// Adjust based on backtesting and performance analysis
const SPREAD_THRESHOLD: f64 = 0.05;

// Function to calculate volume order imbalance (VOI)
fn calculate_voi(order_book: &OrderBook) -> (f64, f64, f64) {
    let bid_volume = order_book.bids.levels.iter().map(|bid| bid.amount).sum();
    let ask_volume = order_book.asks.levels.iter().map(|ask| ask.amount).sum();
    let voi = bid_volume - ask_volume;
    (voi, bid_volume, ask_volume)
}

// Function to determine if a trade should be made
fn should_trade(spread: f64, voi: f64) -> bool {
    spread <= SPREAD_THRESHOLD && voi.abs() > 0.0
}

fn create_limit_sell_order(symbol: &str, trade_size: f64, ask: f64) {
    info!(
        "Selling {} {} at {} at {}",
        trade_size,
        symbol,
        ask,
        Utc::now()
    );
}

fn create_limit_buy_order(symbol: &str, trade_size: f64, bid: f64) {
    info!(
        "Buying {} {} at {} at {}",
        trade_size,
        symbol,
        bid,
        Utc::now()
    );
}

#[tokio::main]
async fn main() {
    init_logging();

    let mut positions: Vec<f64> = Vec::new();
    let mut cash: f64 = 0.0;
    let symbol = "BTC/USDT";

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
        let spread: f64 = (ask - bid) / bid * 100.0;
        let last_price: f64 = (bid + ask) / 2.0;

        // Calculate volume order imbalance
        let (voi, bid_volume, ask_volume) = calculate_voi(&order_book);

        // Calculate Order Imbalance Ratio (OIR)
        let oir: f64 = (bid_volume - ask_volume) / (bid_volume + ask_volume);

        // Calculate Mid-Price Basis (MPB)
        let mpb: f64 = last_price - ((bid + ask) / 2.0);

        // Check if a trade should be made
        if should_trade(spread, voi) {
            // Buy at the bid price if VOI is positive and OIR indicates a strong buy signal
            if voi > 0.0 && oir > 0.1 {
                create_limit_buy_order(symbol, TRADE_SIZE, bid);
                positions.push(TRADE_SIZE);
                cash -= bid * TRADE_SIZE;
                info!(
                    "Bought {} {} at {} at {}",
                    TRADE_SIZE,
                    symbol,
                    bid,
                    Utc::now()
                );
            }
            // Sell at the ask price if VOI is negative and MPB indicates a strong sell signal
            else if voi < 0.0 && mpb < -0.1 && !positions.is_empty() {
                create_limit_sell_order(symbol, TRADE_SIZE, ask);
                positions.pop();
                cash += ask * TRADE_SIZE;
                info!(
                    "Sold {} {} at {} at {}",
                    TRADE_SIZE,
                    symbol,
                    ask,
                    Utc::now()
                );
            }
        }

        // Calculate the current portfolio value
        let portfolio_value = cash + positions.len() as f64 * bid;
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
