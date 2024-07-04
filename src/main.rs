use barter_data::subscription::book::Level;
use barter_data::subscription::book::OrderBook;
use barter_data::subscription::book::OrderBookSide;
use barter_integration::model::Side;
use chrono::Utc;
use std::thread;
use std::time::Duration;

// Example trade size in BTC
const TRADE_SIZE: f64 = 0.01;

// Spread threshold for making a trade in percentage
const SPREAD_THRESHOLD: f64 = 0.01;

// Function to calculate volume order imbalance (VOI)
fn calculate_voi(_order_book: &OrderBook) -> (f64, f64, f64) {
    // let bid_volume: f64 = order_book.bids.levels.iter().map(|bid| bid.amount).sum();
    // let ask_volume: f64 = order_book.asks.levels.iter().map(|ask| ask.amount).sum();
    let bid_volume = 0_f64;
    let ask_volume = 0_f64;
    let voi: f64 = bid_volume - ask_volume;
    (voi, bid_volume, ask_volume)
}

// Function to determine if a trade should be made
fn should_trade(spread: f64, voi: f64) -> bool {
    spread <= SPREAD_THRESHOLD && voi.abs() > 0.0
}

fn create_limit_sell_order(symbol: &str, trade_size: f64, ask: f64) {
    println!(
        "Selling {} {} at {} at {}",
        trade_size,
        symbol,
        ask,
        Utc::now()
    );
}

fn create_limit_buy_order(symbol: &str, trade_size: f64, bid: f64) {
    println!(
        "Buying {} {} at {} at {}",
        trade_size,
        symbol,
        bid,
        Utc::now()
    );
}

fn fetch_order_book(_symbol: &str) -> OrderBook {
    OrderBook {
        last_update_time: chrono::Utc::now(),
        bids: OrderBookSide::new(
            Side::Buy,
            vec![Level {
                price: 10000.0,
                amount: 1.0,
            }],
        ),
        asks: OrderBookSide::new(
            Side::Sell,
            vec![Level {
                price: 10001.0,
                amount: 1.0,
            }],
        ),
    }
}

fn main() {
    let mut positions: Vec<f64> = Vec::new();
    let mut cash: f64 = 0.0;
    let symbol = "BTC/USDT";

    // Main trading loop
    loop {
        let order_book: OrderBook = fetch_order_book(symbol);

        let bid: f64 = 0_f64; // price
        let ask: f64 = 0_f64; // price
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
                // Hypothetical function to create a limit buy order
                barter_data::create_limit_buy_order(symbol, TRADE_SIZE, bid);
                positions.push(TRADE_SIZE);
                cash -= bid * TRADE_SIZE;
                println!(
                    "Bought {} {} at {} at {}",
                    TRADE_SIZE,
                    symbol,
                    bid,
                    Utc::now()
                );
            }

            // Sell at the ask price if VOI is negative and MPB indicates a strong sell signal
            else if voi < 0.0 && mpb < -0.1 && !positions.is_empty() {
                // Hypothetical function to create a limit sell order
                barter_data::create_limit_sell_order(symbol, TRADE_SIZE, ask);
                positions.pop();
                cash += ask * TRADE_SIZE;
                println!(
                    "Sold {} {} at {} at {}",
                    TRADE_SIZE,
                    symbol,
                    ask,
                    Utc::now()
                );
            }
        }

        // Calculate the current portfolio value
        let portfolio_value: f64 = cash + positions.len() as f64 * bid;
        println!("Current portfolio value: ${:.2}", portfolio_value);

        // Sleep before the next iteration
        thread::sleep(Duration::from_secs(1));
    }
}
