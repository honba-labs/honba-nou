//! Numerical checks for analytics primitives.

use chrono::{DateTime, Utc};
use honba_analytics::{AnalyticsError, EquityStats, PerformanceReport, RoundTrip, TradeStats};
use honba_core::uuid::Uuid;
use honba_core::{Currency, Exchange, FillReport, InstrumentId, Money, OrderSide, Price, Quantity};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use rust_decimal_macros::dec;

fn id() -> InstrumentId {
    InstrumentId::new(Exchange::Nse, "X")
}

fn ts(n: u64) -> DateTime<Utc> {
    DateTime::from_timestamp_nanos(n as i64)
}

fn trip(entry: f64, exit: f64, qty: f64) -> RoundTrip {
    RoundTrip::new(
        id(),
        OrderSide::Buy,
        Quantity(Decimal::from_f64_retain(qty).unwrap()),
        Price(Decimal::from_f64_retain(entry).unwrap()),
        Price(Decimal::from_f64_retain(exit).unwrap()),
        ts(1),
        ts(2),
        dec!(0),
    )
}

// --- TradeStats ---

#[test]
fn trade_stats_counts_wins_and_losses() {
    let trades = vec![
        trip(100.0, 110.0, 1.0), // +10
        trip(100.0, 105.0, 1.0), // +5
        trip(100.0, 95.0, 1.0),  // -5
        trip(100.0, 90.0, 1.0),  // -10
    ];
    let s = TradeStats::from_round_trips(&trades).unwrap();
    assert_eq!(s.n_trades, 4);
    assert_eq!(s.n_wins, 2);
    assert_eq!(s.n_losses, 2);
    assert_eq!(s.n_flat, 0);
    assert!((s.win_rate - 0.5).abs() < 1e-12);
    assert!((s.gross_profit - 15.0).abs() < 1e-9);
    assert!((s.gross_loss - 15.0).abs() < 1e-9);
    assert!((s.profit_factor.unwrap() - 1.0).abs() < 1e-9);
    assert!((s.total_pnl).abs() < 1e-9);
}

#[test]
fn trade_stats_all_wins_reports_none_profit_factor() {
    let trades = vec![trip(100.0, 110.0, 1.0), trip(100.0, 105.0, 1.0)];
    let s = TradeStats::from_round_trips(&trades).unwrap();
    assert_eq!(s.n_losses, 0);
    assert!(s.profit_factor.is_none());
    assert!(s.avg_loss.is_none());
    assert!(s.avg_win.is_some());
}

#[test]
fn trade_stats_empty_input_errors() {
    match TradeStats::from_round_trips(&[]) {
        Err(AnalyticsError::EmptyInput) => {}
        other => panic!("expected EmptyInput, got {other:?}"),
    }
}

#[test]
fn round_trip_from_fills_pairs_buy_then_sell() {
    let buy = FillReport::new(
        "F1",
        "B1",
        Uuid::new_v4(),
        id(),
        OrderSide::Buy,
        Price(dec!(100)),
        Quantity(dec!(10)),
        "ALGO_TEST",
        ts(1),
    )
    .with_fee(Money::new(dec!(0.5), Currency::Inr));
    let sell = FillReport::new(
        "F2",
        "S1",
        Uuid::new_v4(),
        id(),
        OrderSide::Sell,
        Price(dec!(110)),
        Quantity(dec!(10)),
        "ALGO_TEST",
        ts(2),
    )
    .with_fee(Money::new(dec!(0.5), Currency::Inr));
    let rt = RoundTrip::from_fills(&buy, &sell).unwrap();
    assert_eq!(rt.side, OrderSide::Buy);
    assert!((rt.gross_pnl.to_f64().unwrap() - 100.0).abs() < 1e-9);
    assert!((rt.fees.to_f64().unwrap() - 1.0).abs() < 1e-9);
    assert!((rt.net_pnl.to_f64().unwrap() - 99.0).abs() < 1e-9);
}

#[test]
fn round_trip_from_fills_short_profits_on_decline() {
    let sell = FillReport::new(
        "F1",
        "S1",
        Uuid::new_v4(),
        id(),
        OrderSide::Sell,
        Price(dec!(110)),
        Quantity(dec!(10)),
        "ALGO_TEST",
        ts(1),
    );
    let buy = FillReport::new(
        "F2",
        "B1",
        Uuid::new_v4(),
        id(),
        OrderSide::Buy,
        Price(dec!(100)),
        Quantity(dec!(10)),
        "ALGO_TEST",
        ts(2),
    );
    let rt = RoundTrip::from_fills(&sell, &buy).unwrap();
    assert_eq!(rt.side, OrderSide::Sell);
    assert!((rt.gross_pnl.to_f64().unwrap() - 100.0).abs() < 1e-9);
}

fn fill(side: OrderSide, n: u64, costs: Money) -> FillReport {
    FillReport::new(
        format!("F{n}"),
        format!("T{n}"),
        Uuid::new_v4(),
        id(),
        side,
        Price(dec!(100)),
        Quantity(dec!(10)),
        "ALGO_TEST",
        ts(n),
    )
    .with_fee(costs)
}

#[test]
fn round_trip_from_fills_rejects_fee_currency_mismatch() {
    let entry = fill(OrderSide::Buy, 1, Money::new(dec!(50), Currency::Inr));
    let exit = fill(OrderSide::Sell, 2, Money::new(dec!(50), Currency::Usd));
    match RoundTrip::from_fills(&entry, &exit) {
        Err(AnalyticsError::TradeMismatch(msg)) => {
            assert!(msg.contains("cannot sum fees"), "{msg}")
        }
        other => panic!("expected TradeMismatch, got {other:?}"),
    }
}

#[test]
fn round_trip_from_fills_rejects_fee_overflow() {
    let entry = fill(OrderSide::Buy, 1, Money::new(Decimal::MAX, Currency::Inr));
    let exit = fill(OrderSide::Sell, 2, Money::new(dec!(1), Currency::Inr));
    match RoundTrip::from_fills(&entry, &exit) {
        Err(AnalyticsError::TradeMismatch(msg)) => {
            assert!(msg.contains("cannot sum fees"), "{msg}")
        }
        other => panic!("expected TradeMismatch, got {other:?}"),
    }
}

// --- EquityStats ---

#[test]
fn equity_stats_total_return_from_returns() {
    let returns = [0.10, 0.05];
    let s = EquityStats::from_returns(&returns, 252.0, 0.0).unwrap();
    // (1.10 * 1.05) - 1 = 0.155
    assert!((s.total_return - 0.155).abs() < 1e-9);
}

#[test]
fn equity_stats_total_return_from_curve() {
    let curve = [100.0, 110.0, 121.0];
    let s = EquityStats::from_equity_curve(&curve, 252.0, 0.0).unwrap();
    assert!((s.total_return - 0.21).abs() < 1e-9);
}

#[test]
fn equity_stats_max_drawdown() {
    let curve = [100.0, 90.0, 95.0, 85.0, 100.0];
    let s = EquityStats::from_equity_curve(&curve, 252.0, 0.0).unwrap();
    // Peak 100 -> trough 85: dd = 15, dd_pct = 0.15
    assert!((s.max_drawdown_pct - 0.15).abs() < 1e-9);
}

#[test]
fn equity_stats_zero_variance_has_no_sharpe() {
    let returns = [0.01, 0.01, 0.01, 0.01];
    let s = EquityStats::from_returns(&returns, 252.0, 0.0).unwrap();
    assert!(s.sharpe.is_none());
}

#[test]
fn equity_stats_sharpe_sign_follows_mean() {
    let positive = [0.01, 0.02, 0.015, 0.005];
    let negative = [-0.01, -0.02, -0.015, -0.005];
    let sp = EquityStats::from_returns(&positive, 252.0, 0.0).unwrap();
    let sn = EquityStats::from_returns(&negative, 252.0, 0.0).unwrap();
    assert!(sp.sharpe.unwrap() > 0.0);
    assert!(sn.sharpe.unwrap() < 0.0);
}

#[test]
fn equity_stats_insufficient_data_errors() {
    match EquityStats::from_equity_curve(&[100.0], 252.0, 0.0) {
        Err(AnalyticsError::InsufficientData { needed, got }) => {
            assert_eq!(needed, 2);
            assert_eq!(got, 1);
        }
        other => panic!("expected InsufficientData, got {other:?}"),
    }
}

// --- PerformanceReport ---

#[test]
fn performance_report_combines_both() {
    let trades = vec![trip(100.0, 110.0, 1.0), trip(100.0, 95.0, 1.0)];
    let returns = [0.01, -0.005, 0.02];
    let r = PerformanceReport::from_returns(&trades, &returns, 252.0, 0.0).unwrap();
    assert_eq!(r.trades.n_trades, 2);
    assert_eq!(r.equity.n_periods, 3);
}
