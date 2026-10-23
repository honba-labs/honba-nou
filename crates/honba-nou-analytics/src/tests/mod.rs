//! Unit tests for this crate, one file per area.

mod dsr;
mod equity_stats;
mod error;
mod math;
mod monte_carlo;
mod pbo;
mod report;
mod round_trip;
mod trade_stats;
mod validation;
mod walk_forward;

use chrono::{DateTime, Utc};
use honba_core::{Exchange, InstrumentId, OrderSide, Price, Quantity};
use rust_decimal::Decimal;

use crate::RoundTrip;

/// A placeholder instrument for tests where the instrument does not matter.
fn any_instrument() -> InstrumentId {
    InstrumentId::new(Exchange::Nse, "X")
}

/// Helper to create a `DateTime<Utc>` from nanoseconds since the Unix epoch.
fn ts_nanos(nanos: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_nanos(nanos)
}

/// A long round trip of `qty` from `entry` to `exit` with `fees`, held 1..2.
fn long_trip(entry: f64, exit: f64, qty: f64, fees: f64) -> RoundTrip {
    RoundTrip::new(
        any_instrument(),
        OrderSide::Buy,
        Quantity(Decimal::from_f64_retain(qty).unwrap()),
        Price(Decimal::from_f64_retain(entry).unwrap()),
        Price(Decimal::from_f64_retain(exit).unwrap()),
        ts_nanos(1),
        ts_nanos(2),
        Decimal::from_f64_retain(fees).unwrap(),
    )
}

fn assert_close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-12, "{a} != {b}");
}
