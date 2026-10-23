//! Unit tests for `crate::round_trip`.

use honba_core::uuid::Uuid;
use honba_core::{Currency, Exchange, FillReport, InstrumentId, Money, OrderSide, Price, Quantity};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use rust_decimal_macros::dec;

use super::{any_instrument, assert_close, long_trip, ts_nanos};
use crate::{AnalyticsError, RoundTrip};

fn fill(side: OrderSide, qty: f64, price: f64, ts: u64) -> FillReport {
    FillReport::new(
        format!("F-{ts}"),
        format!("O-{ts}"),
        Uuid::new_v4(),
        any_instrument(),
        side,
        Price(Decimal::from_f64_retain(price).unwrap()),
        Quantity(Decimal::from_f64_retain(qty).unwrap()),
        "ALGO_TEST",
        ts_nanos(ts as i64),
    )
}

fn fill_with_costs(side: OrderSide, qty: f64, price: f64, ts: u64, costs: f64) -> FillReport {
    fill(side, qty, price, ts).with_fee(Money::new(
        Decimal::from_f64_retain(costs).unwrap(),
        Currency::Inr,
    ))
}

#[test]
fn new_long_computes_gross_net_and_pct() {
    let t = long_trip(100.0, 110.0, 2.0, 5.0);
    assert_close(t.gross_pnl.to_f64().unwrap(), 20.0);
    assert_close(t.net_pnl.to_f64().unwrap(), 15.0);
    assert_close(t.pnl_pct, 15.0 / 200.0);
}

#[test]
fn new_short_profits_when_price_falls() {
    let t = RoundTrip::new(
        any_instrument(),
        OrderSide::Sell,
        Quantity(dec!(3)),
        Price(dec!(50)),
        Price(dec!(40)),
        ts_nanos(1),
        ts_nanos(2),
        dec!(0),
    );
    assert_close(t.gross_pnl.to_f64().unwrap(), 30.0);
    assert_close(t.net_pnl.to_f64().unwrap(), 30.0);
}

#[test]
fn zero_entry_notional_gives_zero_pct() {
    assert_eq!(long_trip(0.0, 10.0, 1.0, 0.0).pnl_pct, 0.0);
}

#[test]
fn duration_saturates_at_zero_when_exit_precedes_entry() {
    let mut t = long_trip(1.0, 2.0, 1.0, 0.0);
    assert_eq!(t.duration_nanos(), 1);
    t.exit_ts = ts_nanos(0);
    assert_eq!(t.duration_nanos(), 0);
}

#[test]
fn from_fills_sums_fees_and_takes_entry_metadata() {
    // Fees are read off the trades now: 1.00 + 2.00 = 3.00.
    let entry = fill_with_costs(OrderSide::Buy, 2.0, 100.0, 10, 1.0);
    let exit = fill_with_costs(OrderSide::Sell, 2.0, 105.0, 20, 2.0);
    let t = RoundTrip::from_fills(&entry, &exit).unwrap();
    assert_eq!(t.side, OrderSide::Buy);
    assert_eq!(t.instrument_id, any_instrument());
    assert_eq!(
        (
            t.entry_ts.timestamp_nanos_opt().unwrap(),
            t.exit_ts.timestamp_nanos_opt().unwrap()
        ),
        (10, 20)
    );
    assert_close(t.fees.to_f64().unwrap(), 3.0);
    assert_close(t.gross_pnl.to_f64().unwrap(), 10.0);
    assert_close(t.net_pnl.to_f64().unwrap(), 7.0);
}

#[test]
fn from_fills_rejects_fees_in_different_currencies() {
    let entry = fill_with_costs(OrderSide::Buy, 1.0, 100.0, 1, 1.0);
    let exit = FillReport::new(
        "F-2",
        "O-2",
        Uuid::new_v4(),
        any_instrument(),
        OrderSide::Sell,
        Price(dec!(101)),
        Quantity(dec!(1)),
        "ALGO_TEST",
        ts_nanos(2),
    )
    .with_fee(Money::new(dec!(1), Currency::Usd));
    assert!(matches!(
        RoundTrip::from_fills(&entry, &exit),
        Err(AnalyticsError::TradeMismatch(_))
    ));
}

#[test]
fn from_fills_rejects_fee_overflow() {
    let entry =
        fill(OrderSide::Buy, 1.0, 100.0, 1).with_fee(Money::new(Decimal::MAX, Currency::Inr));
    let exit = fill(OrderSide::Sell, 1.0, 101.0, 2).with_fee(Money::new(dec!(1), Currency::Inr));
    assert!(matches!(
        RoundTrip::from_fills(&entry, &exit),
        Err(AnalyticsError::TradeMismatch(_))
    ));
}

#[test]
fn from_fills_rejects_same_side_pairs() {
    let a = fill(OrderSide::Buy, 1.0, 100.0, 1);
    let b = fill(OrderSide::Buy, 1.0, 101.0, 2);
    assert!(matches!(
        RoundTrip::from_fills(&a, &b),
        Err(AnalyticsError::TradeMismatch(_))
    ));
}

#[test]
fn from_fills_rejects_different_instruments() {
    let entry = fill(OrderSide::Buy, 1.0, 100.0, 1);
    let exit = FillReport::new(
        "F-2",
        "O-2",
        Uuid::new_v4(),
        InstrumentId::new(Exchange::Nse, "Y"),
        OrderSide::Sell,
        Price(dec!(101)),
        Quantity(dec!(1)),
        "ALGO_TEST",
        ts_nanos(2),
    );
    let err = RoundTrip::from_fills(&entry, &exit).unwrap_err();
    assert!(err.to_string().contains("different instruments"), "{err}");
}
