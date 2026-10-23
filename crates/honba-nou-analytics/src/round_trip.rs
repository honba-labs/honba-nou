//! A completed round-trip trade: entry and exit paired.

use chrono::{DateTime, Utc};
use honba_core::{FillReport, InstrumentId, OrderSide, Price, Quantity};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use crate::error::{AnalyticsError, Result};

/// A closed position: an entry and exit paired.
///
/// Analytics operate on round trips, not raw fills. Pairing fills into round
/// trips is the caller's job (or a dedicated module), since the matching
/// rule varies — FIFO, LIFO, or average cost.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoundTrip {
    /// The instrument traded.
    pub instrument_id: InstrumentId,
    /// Direction of the position.
    pub side: OrderSide,
    /// Quantity entered.
    pub quantity: Quantity,
    /// Entry price.
    pub entry_price: Price,
    /// Exit price.
    pub exit_price: Price,
    /// Entry timestamp.
    pub entry_ts: DateTime<Utc>,
    /// Exit timestamp.
    pub exit_ts: DateTime<Utc>,
    /// Realized profit and loss, before fees.
    pub gross_pnl: Decimal,
    /// Fees paid, positive magnitude.
    pub fees: Decimal,
    /// Realized profit and loss, net of fees.
    pub net_pnl: Decimal,
    /// Net PnL as a fraction of entry notional.
    pub pnl_pct: f64,
}

impl RoundTrip {
    /// Pairs an entry fill and an exit fill into a round trip.
    ///
    /// The entry fill's side determines the direction. The exit fill must be
    /// the opposite side and reference the same instrument. Fees are read off
    /// the fills themselves.
    pub fn from_fills(entry: &FillReport, exit: &FillReport) -> Result<Self> {
        if entry.instrument != exit.instrument {
            return Err(AnalyticsError::TradeMismatch(
                "entry and exit reference different instruments".into(),
            ));
        }

        let (side, quantity, entry_price, exit_price) = match (entry.side, exit.side) {
            (OrderSide::Buy, OrderSide::Sell) => {
                (OrderSide::Buy, entry.quantity, entry.price, exit.price)
            }
            (OrderSide::Sell, OrderSide::Buy) => {
                (OrderSide::Sell, entry.quantity, entry.price, exit.price)
            }
            _ => {
                return Err(AnalyticsError::TradeMismatch(
                    "entry and exit must be opposite sides".into(),
                ));
            }
        };

        if entry.fee.currency != exit.fee.currency {
            return Err(AnalyticsError::TradeMismatch(format!(
                "cannot sum fees in different currencies ({} vs {})",
                entry.fee.currency, exit.fee.currency
            )));
        }

        let fees = entry
            .fee
            .amount
            .checked_add(exit.fee.amount)
            .ok_or_else(|| {
                AnalyticsError::TradeMismatch("cannot sum fees: decimal overflow".into())
            })?;

        let gross_pnl = match side {
            OrderSide::Buy => (exit_price.0 - entry_price.0) * quantity.0,
            OrderSide::Sell => (entry_price.0 - exit_price.0) * quantity.0,
        };

        let net_pnl = gross_pnl - fees;
        let entry_notional = entry_price.0 * quantity.0;
        let pnl_pct = if entry_notional != dec!(0) {
            (net_pnl / entry_notional).try_into().unwrap_or(0.0)
        } else {
            0.0
        };

        Ok(Self {
            instrument_id: entry.instrument.clone(),
            side,
            quantity,
            entry_price,
            exit_price,
            entry_ts: entry.timestamp,
            exit_ts: exit.timestamp,
            gross_pnl,
            fees,
            net_pnl,
            pnl_pct,
        })
    }

    /// Constructs a round trip directly from explicit values.
    ///
    /// Useful in tests and when importing external trade logs.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instrument_id: InstrumentId,
        side: OrderSide,
        quantity: Quantity,
        entry_price: Price,
        exit_price: Price,
        entry_ts: DateTime<Utc>,
        exit_ts: DateTime<Utc>,
        fees: Decimal,
    ) -> Self {
        let gross_pnl = match side {
            OrderSide::Buy => (exit_price.0 - entry_price.0) * quantity.0,
            OrderSide::Sell => (entry_price.0 - exit_price.0) * quantity.0,
        };
        let net_pnl = gross_pnl - fees;
        let entry_notional = entry_price.0 * quantity.0;
        let pnl_pct = if entry_notional != dec!(0) {
            (net_pnl / entry_notional).try_into().unwrap_or(0.0)
        } else {
            0.0
        };
        Self {
            instrument_id,
            side,
            quantity,
            entry_price,
            exit_price,
            entry_ts,
            exit_ts,
            gross_pnl,
            fees,
            net_pnl,
            pnl_pct,
        }
    }

    /// Returns the holding period in nanoseconds.
    pub fn duration_nanos(&self) -> u64 {
        (self.exit_ts - self.entry_ts)
            .num_nanoseconds()
            .unwrap_or(0)
            .max(0) as u64
    }
}
