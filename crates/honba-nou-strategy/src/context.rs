//! The [`StrategyContext`] port: a strategy's only view of the world (ADR 008).
//!
//! A strategy reads the clock, its positions, cash and instrument metadata,
//! and submits [`OrderIntent`]s through the context. It never touches the
//! execution engine, I/O or the wall clock, so the same strategy runs
//! unchanged in backtest, paper and live. Mirrors the Python
//! `honba.strategies.context.StrategyContext` ABC.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use honba_core::{Currency, InstrumentId, Money, OrderSide, Price, Quantity};
use rust_decimal::Decimal;

use crate::intent::OrderIntent;

/// What a strategy may read and do. Implementations are supplied by the
/// runner; every [`Strategy`](crate::Strategy) hook receives one as
/// `&mut dyn StrategyContext`.
pub trait StrategyContext {
    /// The `ts_init` of the event being processed; zero before the first event.
    fn now(&self) -> DateTime<Utc>;

    /// Net signed quantity held (positive long, negative short), updated from fills.
    fn position(&self, instrument_id: &InstrumentId) -> Decimal;

    /// Every non-flat position, ordered by instrument id (symbol, then exchange).
    fn positions(&self) -> Vec<(InstrumentId, Decimal)>;

    /// Initial cash plus the net cash flow of all fills (buys debit
    /// notional plus costs, sells credit notional minus costs), in minor units.
    fn cash(&self) -> Money;

    /// `true` while an intent submitted for `instrument_id` is not fully
    /// filled or rejected.
    fn busy(&self, instrument_id: &InstrumentId) -> bool;

    /// Instrument metadata (lot and tick size), if the run knows it.
    fn instrument(&self, instrument_id: &InstrumentId) -> Option<&InstrumentInfo>;

    /// Queues an intent; the runner turns it into an order after the current
    /// hook returns.
    fn submit(&mut self, intent: OrderIntent);
}

/// Instrument metadata (lot and tick size).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstrumentInfo {
    pub id: InstrumentId,
    pub lot_size: Quantity,
    pub tick_size: Price,
}

/// Unfilled quantity below this counts as filled (float noise from partial fills).
const EPSILON: Decimal = Decimal::ZERO;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Pending {
    buy: Decimal,
    sell: Decimal,
}

/// The reference [`StrategyContext`]: a deterministic in-memory ledger.
///
/// The runner sets the clock, applies fills and releases rejected intents;
/// the strategy reads it and submits intents, which the runner drains. It is
/// pure (no I/O, no wall clock), so backtest, paper and live share it.
/// Mirrors the Python `honba.strategies.context.LedgerContext`.
#[derive(Clone, Debug)]
pub struct LedgerContext {
    now: DateTime<Utc>,
    cash: Money,
    currency: Currency,
    positions: BTreeMap<InstrumentId, Decimal>,
    pending: BTreeMap<InstrumentId, Pending>,
    instruments: BTreeMap<InstrumentId, InstrumentInfo>,
    outbox: Vec<OrderIntent>,
}

impl Default for LedgerContext {
    fn default() -> Self {
        Self {
            now: DateTime::<Utc>::MIN,
            cash: Money::zero(Currency::Inr),
            currency: Currency::Inr,
            positions: Default::default(),
            pending: Default::default(),
            instruments: Default::default(),
            outbox: Default::default(),
        }
    }
}

impl LedgerContext {
    /// An empty ledger with zero cash, in the given currency.
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty ledger starting with `cash`.
    pub fn with_cash(cash: Money) -> Self {
        Self {
            currency: cash.currency,
            cash,
            ..Self::default()
        }
    }

    /// The currency this ledger settles in.
    pub fn currency(&self) -> Currency {
        self.currency
    }

    /// Sets the clock to the `ts_init` of the event about to be processed.
    pub fn set_now(&mut self, ts_init: DateTime<Utc>) {
        self.now = ts_init;
    }

    /// Registers instrument metadata for [`StrategyContext::instrument`].
    pub fn add_instrument(&mut self, instrument: InstrumentInfo) {
        self.instruments.insert(instrument.id.clone(), instrument);
    }

    /// Sets the signed position of `instrument_id` without booking a fill: no cash moves.
    /// For runs that start from an existing portfolio (for example a reduce-only run).
    pub fn seed_position(&mut self, instrument_id: &InstrumentId, quantity: Decimal) {
        self.positions.insert(instrument_id.clone(), quantity);
    }

    /// Returns and clears submitted intents, in submission order.
    pub fn drain_intents(&mut self) -> Vec<OrderIntent> {
        std::mem::take(&mut self.outbox)
    }

    /// Books a fill: position, cash (notional plus costs) and the pending
    /// quantity of the instrument.
    ///
    /// The notional rounds to minor units once, at the point of booking, and
    /// the costs are already integer `Money` (ADR 0011), so cash accumulates
    /// integers. Booking is all-or-nothing: a fill whose notional cannot be
    /// represented, whose costs are in another currency, or that would
    /// overflow the balance is refused with an error and leaves the ledger
    /// untouched, rather than moving the position without the cash.
    pub fn apply_fill(&mut self, fill: &FillReport) -> Result<()> {
        let notional = (fill.price.0 * fill.quantity.0).min(Decimal::MAX);
        let notional_money = Money::new(notional, self.currency);
        let costs = fill.fee_breakdown.parse::<Decimal>().unwrap_or(Decimal::ZERO);
        let costs_money = Money::new(costs, self.currency);

        let cash = if fill.side == OrderSide::Buy {
            self.cash - (notional_money + costs_money)?
        } else {
            self.cash + (notional_money - costs_money)?
        };
        let signed = if fill.side == OrderSide::Buy {
            fill.quantity
        } else {
            -fill.quantity
        };
        self.cash = cash;
        *self
            .positions
            .entry(fill.instrument.clone())
            .or_insert(Decimal::ZERO) += signed;
        self.reduce_pending(&fill.instrument, fill.side, fill.quantity);
        Ok(())
    }

    /// An intent was rejected or its order cancelled unfilled: it no longer
    /// counts towards [`StrategyContext::busy`].
    ///
    /// An intent that fails [`OrderIntent::validate`] never counted (see
    /// `submit`), so releasing it is a no-op.
    pub fn release(&mut self, intent: &OrderIntent) {
        if intent.validate().is_err() {
            return;
        }
        self.reduce_pending(&intent.instrument_id, intent.side, intent.quantity);
    }

    /// Releases `quantity` of the pending `side` quantity of `instrument_id`:
    /// the unfilled remainder of an order the execution port rejected or
    /// cancelled. A non-positive or non-finite quantity is a no-op.
    pub fn release_remainder(
        &mut self,
        instrument_id: &InstrumentId,
        side: OrderSide,
        quantity: Decimal,
    ) {
        if quantity > Decimal::ZERO {
            self.reduce_pending(instrument_id, side, quantity);
        }
    }

    fn reduce_pending(&mut self, instrument_id: &InstrumentId, side: OrderSide, qty: Decimal) {
        let Some(p) = self.pending.get_mut(instrument_id) else {
            return;
        };
        let slot = if side == OrderSide::Buy {
            &mut p.buy
        } else {
            &mut p.sell
        };
        let left = *slot - qty;
        *slot = if left > EPSILON { left } else { Decimal::ZERO };
        if *p == Pending::default() {
            self.pending.remove(instrument_id);
        }
    }
}

impl StrategyContext for LedgerContext {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }

    fn position(&self, instrument_id: &InstrumentId) -> Decimal {
        self.positions.get(instrument_id).copied().unwrap_or(Decimal::ZERO)
    }

    fn positions(&self) -> Vec<(InstrumentId, Decimal)> {
        self.positions
            .iter()
            .filter(|(_, q)| **q != Decimal::ZERO)
            .map(|(id, q)| (id.clone(), *q))
            .collect()
    }

    fn cash(&self) -> Money {
        self.cash
    }

    fn busy(&self, instrument_id: &InstrumentId) -> bool {
        self.pending
            .get(instrument_id)
            .is_some_and(|p| p.buy > Decimal::ZERO || p.sell > Decimal::ZERO)
    }

    fn instrument(&self, instrument_id: &InstrumentId) -> Option<&InstrumentInfo> {
        self.instruments.get(instrument_id)
    }

    fn submit(&mut self, intent: OrderIntent) {
        // An invalid intent is still queued so the runner can reject it, but
        // it must not touch the pending ledger (a NaN would corrupt it).
        if intent.validate().is_ok() {
            let p = self
                .pending
                .entry(intent.instrument_id.clone())
                .or_default();
            if intent.side == OrderSide::Buy {
                p.buy += intent.quantity;
            } else {
                p.sell += intent.quantity;
            }
        }
        self.outbox.push(intent);
    }
}

/// A [`StrategyContext`] implementation backed directly by a [`StateCache`].
/// This is kept for compatibility but requires honba-engine which is deprecated.
#[cfg(feature = "engine")]
#[derive(Debug)]
pub struct CacheContext<'a> {
    cache: &'a honba_engine::StateCache,
    now: DateTime<Utc>,
    cash: Money,
    currency: Currency,
    outbox: Vec<OrderIntent>,
}

#[cfg(feature = "engine")]
impl<'a> CacheContext<'a> {
    /// Creates a context reading from `cache` with initial `cash`.
    pub fn new(cache: &'a honba_engine::StateCache, cash: Money) -> Self {
        Self {
            cache,
            now: cache.last_ts(),
            currency: cash.currency,
            cash,
            outbox: Vec::new(),
        }
    }

    /// Sets the context clock.
    pub fn set_now(&mut self, now: DateTime<Utc>) {
        self.now = now;
    }

    /// The currency of this context.
    pub fn currency(&self) -> Currency {
        self.currency
    }

    /// Drains submitted intents.
    pub fn drain_intents(&mut self) -> Vec<OrderIntent> {
        std::mem::take(&mut self.outbox)
    }
}

#[cfg(feature = "engine")]
impl<'a> StrategyContext for CacheContext<'a> {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }

    fn position(&self, instrument_id: &InstrumentId) -> Decimal {
        self.cache.position(instrument_id)
    }

    fn positions(&self) -> Vec<(InstrumentId, Decimal)> {
        let mut list: Vec<(InstrumentId, Decimal)> = self
            .cache
            .positions()
            .iter()
            .filter(|(_, qty)| qty.abs() > EPSILON)
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        list.sort_by(|a, b| {
            a.0.symbol()
                .cmp(b.0.symbol())
                .then_with(|| a.0.exchange().as_str().cmp(b.0.exchange().as_str()))
        });
        list
    }

    fn cash(&self) -> Money {
        self.cash
    }

    fn busy(&self, instrument_id: &InstrumentId) -> bool {
        self.cache
            .open_orders()
            .iter()
            .any(|o| &o.instrument_id == instrument_id)
    }

    fn instrument(&self, instrument_id: &InstrumentId) -> Option<&InstrumentInfo> {
        self.cache.instrument(instrument_id)
    }

    fn submit(&mut self, intent: OrderIntent) {
        self.outbox.push(intent);
    }
}