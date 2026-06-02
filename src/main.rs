mod accounts;
mod price_level;
use accounts::{AccountsError, AccountsManager};
use price_level::PriceLevel;
use std::collections::{BTreeMap, HashMap};

pub type OrderId = u64;
pub type Price = u64;
pub type Quantity = u64;
pub type UserId = u64;
pub type Symbol = String;
pub const INITIAL_SYMBOLS: [&str; 1] = ["BTC"];

#[derive(Debug, Clone, Copy)]
pub enum Side {
    Bid,
    Ask,
}
#[derive(Debug, Clone)]
pub struct Order {
    id: OrderId,
    user_id: UserId,
    side: Side,
    symbol: Symbol,
    price: Price,
    remaining: Quantity,
}

#[derive(Debug)]
pub struct Trade {
    maker_order_id: OrderId,
    taker_order_id: OrderId,
    maker_user_id: UserId,
    taker_user_id: UserId,
    symbol: Symbol,
    taker_side: Side,
    price: Price,
    buyer_limit_price: Price,
    quantity: Quantity,
    maker_fully_filled: bool,
    taker_fully_filled: bool,
}

#[derive(Debug)]
struct MatchResult {
    resting_order: Option<Order>,
    fills: Vec<Trade>,
}

#[derive(Debug)]
enum OrderError {
    InvalidPrice,
    InvalidQuantity,
}

#[derive(Debug)]
enum ExchangeError {
    Account,
    Order,
    UnknownSymbol,
    UnknownOrder,
    Unauthorized,
}

impl From<AccountsError> for ExchangeError {
    fn from(_error: AccountsError) -> Self {
        ExchangeError::Account
    }
}

impl From<OrderError> for ExchangeError {
    fn from(_error: OrderError) -> Self {
        ExchangeError::Order
    }
}

// priority is first the price and then the oldest order
#[derive(Debug)]
struct Orderbook {
    bids: BTreeMap<Price, PriceLevel>,
    asks: BTreeMap<Price, PriceLevel>,
    open_orders: HashMap<OrderId, Order>,
}

#[derive(Debug)]
struct Exchange {
    orderbooks: HashMap<Symbol, Orderbook>,
    accounts_manager: AccountsManager,
}

impl Exchange {
    fn new() -> Self {
        let mut orderbooks: HashMap<Symbol, Orderbook> = HashMap::new();
        let accounts_manager = AccountsManager::new();

        for symbol in INITIAL_SYMBOLS {
            orderbooks.insert(symbol.to_string(), Orderbook::new());
        }

        Exchange {
            orderbooks,
            accounts_manager,
        }
    }

    fn insert_symbol(&mut self, symbol: Symbol) {
        self.orderbooks.insert(symbol, Orderbook::new());
    }

    fn submit_limit_order(&mut self, order: Order) -> Result<MatchResult, ExchangeError> {
        Orderbook::validate_order(&order)?;

        let symbol = order.symbol.clone();
        if !self.orderbooks.contains_key(&symbol) {
            return Err(ExchangeError::UnknownSymbol);
        }

        self.accounts_manager.is_enough_balance_for_order(&order)?;

        match order.side {
            Side::Bid => self
                .accounts_manager
                .reserve_user_balance_for_order(&order)?,
            Side::Ask => self
                .accounts_manager
                .reserve_user_position_for_order(&order)?,
        }

        let result = self
            .orderbooks
            .get_mut(&symbol)
            .ok_or(ExchangeError::UnknownSymbol)?
            .handle_limit_order(order)?;

        for fill in &result.fills {
            self.accounts_manager.settle_fill(fill)?;
        }

        Ok(result)
    }

    fn cancel_order(
        &mut self,
        user_id: UserId,
        symbol: Symbol,
        order_id: OrderId,
    ) -> Result<Order, ExchangeError> {
        let order = self
            .orderbooks
            .get(&symbol)
            .ok_or(ExchangeError::UnknownSymbol)?
            .get_open_order(order_id)
            .cloned()
            .ok_or(ExchangeError::UnknownOrder)?;

        if order.user_id != user_id {
            return Err(ExchangeError::Unauthorized);
        }

        self.accounts_manager.release_reserved_order(&order)?;

        self.orderbooks
            .get_mut(&symbol)
            .ok_or(ExchangeError::UnknownSymbol)?
            .cancel_order(order_id)
            .ok_or(ExchangeError::UnknownOrder)
    }
}

impl Orderbook {
    fn new() -> Self {
        let bids: BTreeMap<Price, PriceLevel> = BTreeMap::new();
        let asks: BTreeMap<Price, PriceLevel> = BTreeMap::new();
        let open_orders = HashMap::new();
        Orderbook {
            bids,
            asks,
            open_orders,
        }
    }

    // this will be a check before inserting in the orderbook, if this returns true, we do the
    // match, otherwise we just insert it in orderbook
    fn can_match(&self, incoming_order: &Order) -> bool {
        match incoming_order.side {
            Side::Bid => self
                .best_ask()
                .is_some_and(|ask| incoming_order.price >= ask),
            Side::Ask => self
                .best_bid()
                .is_some_and(|bid| incoming_order.price <= bid),
        }
    }

    fn best_bid(&self) -> Option<Price> {
        self.bids.keys().next_back().copied()
    }

    fn best_ask(&self) -> Option<Price> {
        self.asks.keys().next().copied()
    }

    fn get_mut_open_order(&mut self, order_id: OrderId) -> Option<&mut Order> {
        self.open_orders.get_mut(&order_id)
    }

    fn get_open_order(&self, order_id: OrderId) -> Option<&Order> {
        self.open_orders.get(&order_id)
    }

    fn remove_order_id_at_price(
        side: &mut BTreeMap<Price, PriceLevel>,
        price: Price,
        order_id: OrderId,
    ) -> Option<()> {
        let should_remove_price = {
            let price_level = side.get_mut(&price)?;

            if price_level.len() == 1 {
                if price_level.front().ok()? != order_id {
                    return None;
                }
                true
            } else {
                price_level.delete(order_id).ok()?;
                false
            }
        };

        if should_remove_price {
            side.remove(&price);
        }

        Some(())
    }

    fn cancel_order(&mut self, order_id: OrderId) -> Option<Order> {
        let order = self.open_orders.get(&order_id)?.clone();

        match order.side {
            Side::Bid => Self::remove_order_id_at_price(&mut self.bids, order.price, order.id)?,
            Side::Ask => Self::remove_order_id_at_price(&mut self.asks, order.price, order.id)?,
        }

        self.open_orders.remove(&order_id)
    }

    fn pop_front_order_id_at_price(
        side: &mut BTreeMap<Price, PriceLevel>,
        price: Price,
    ) -> Option<OrderId> {
        let should_remove_price;
        let order_id;

        {
            let price_level = side.get_mut(&price)?;

            if price_level.len() == 1 {
                order_id = price_level.front().ok()?;
                should_remove_price = true;
            } else {
                order_id = price_level.pop_front().ok()?;
                should_remove_price = false;
            }
        }

        if should_remove_price {
            side.remove(&price);
        }

        Some(order_id)
    }

    fn validate_order(order: &Order) -> Result<(), OrderError> {
        if order.price == 0 {
            return Err(OrderError::InvalidPrice);
        }

        if order.remaining == 0 {
            return Err(OrderError::InvalidQuantity);
        }

        Ok(())
    }

    fn handle_limit_order(&mut self, incoming_order: Order) -> Result<MatchResult, OrderError> {
        Self::validate_order(&incoming_order)?;

        let mut remaining_order: Order = incoming_order;
        let mut fills: Vec<Trade> = vec![];

        match remaining_order.side {
            Side::Bid => {
                while (remaining_order.remaining > 0) && self.can_match(&remaining_order) {
                    let best_ask_price = self.best_ask().unwrap();
                    // find the best ask for the bidder
                    let matched_resting_ask_order_id =
                        self.asks.get(&best_ask_price).unwrap().front().unwrap();
                    let matched_resting_ask_order = self
                        .get_mut_open_order(matched_resting_ask_order_id)
                        .unwrap();
                    if matched_resting_ask_order.remaining > remaining_order.remaining {
                        let matched_quantity = remaining_order.remaining;
                        remaining_order.remaining -= matched_quantity; // 0 
                        assert!(remaining_order.remaining == 0);
                        matched_resting_ask_order.remaining -= matched_quantity;

                        let trade = Trade {
                            maker_order_id: matched_resting_ask_order.id,
                            taker_order_id: remaining_order.id,
                            maker_user_id: matched_resting_ask_order.user_id,
                            taker_user_id: remaining_order.user_id,
                            symbol: remaining_order.symbol.clone(),
                            taker_side: remaining_order.side,
                            price: matched_resting_ask_order.price,
                            buyer_limit_price: remaining_order.price,
                            quantity: matched_quantity,
                            maker_fully_filled: false,
                            taker_fully_filled: true,
                        };
                        fills.push(trade);
                    } else if matched_resting_ask_order.remaining <= remaining_order.remaining {
                        let matched_quantity = matched_resting_ask_order.remaining;
                        remaining_order.remaining -= matched_quantity;
                        matched_resting_ask_order.remaining -= matched_quantity; // 0
                        assert!(matched_resting_ask_order.remaining == 0);

                        let trade = Trade {
                            maker_order_id: matched_resting_ask_order.id,
                            taker_order_id: remaining_order.id,
                            maker_user_id: matched_resting_ask_order.user_id,
                            taker_user_id: remaining_order.user_id,
                            symbol: remaining_order.symbol.clone(),
                            taker_side: remaining_order.side,
                            price: matched_resting_ask_order.price,
                            buyer_limit_price: remaining_order.price,
                            quantity: matched_quantity,
                            maker_fully_filled: true,
                            taker_fully_filled: remaining_order.remaining == 0,
                        };

                        assert_eq!(
                            Self::pop_front_order_id_at_price(&mut self.asks, best_ask_price),
                            Some(matched_resting_ask_order_id)
                        );
                        assert!(
                            self.open_orders
                                .remove(&matched_resting_ask_order_id)
                                .is_some()
                        );
                        fills.push(trade);
                    };
                }
                let resting_order: Option<Order> = if remaining_order.remaining > 0 {
                    self.insert(remaining_order.clone());
                    Some(remaining_order)
                } else {
                    None
                };
                Ok(MatchResult {
                    resting_order,
                    fills,
                })
            }
            Side::Ask => {
                while (remaining_order.remaining > 0) && self.can_match(&remaining_order) {
                    let best_bid_price = self.best_bid().unwrap();
                    // find the best bid for the asker
                    let matched_resting_bid_order_id =
                        self.bids.get(&best_bid_price).unwrap().front().unwrap();
                    let matched_resting_bid_order = self
                        .get_mut_open_order(matched_resting_bid_order_id)
                        .unwrap();
                    if matched_resting_bid_order.remaining > remaining_order.remaining {
                        let matched_quantity = remaining_order.remaining;
                        remaining_order.remaining -= matched_quantity; // 0
                        assert!(remaining_order.remaining == 0);
                        matched_resting_bid_order.remaining -= matched_quantity;

                        let trade = Trade {
                            maker_order_id: matched_resting_bid_order.id,
                            taker_order_id: remaining_order.id,
                            maker_user_id: matched_resting_bid_order.user_id,
                            taker_user_id: remaining_order.user_id,
                            symbol: remaining_order.symbol.clone(),
                            taker_side: remaining_order.side,
                            price: matched_resting_bid_order.price,
                            buyer_limit_price: matched_resting_bid_order.price,
                            quantity: matched_quantity,
                            maker_fully_filled: false,
                            taker_fully_filled: true,
                        };
                        fills.push(trade);
                    } else if matched_resting_bid_order.remaining <= remaining_order.remaining {
                        let matched_quantity = matched_resting_bid_order.remaining;
                        remaining_order.remaining -= matched_quantity;
                        matched_resting_bid_order.remaining -= matched_quantity; // 0
                        assert!(matched_resting_bid_order.remaining == 0);

                        let trade = Trade {
                            maker_order_id: matched_resting_bid_order.id,
                            taker_order_id: remaining_order.id,
                            maker_user_id: matched_resting_bid_order.user_id,
                            taker_user_id: remaining_order.user_id,
                            symbol: remaining_order.symbol.clone(),
                            taker_side: remaining_order.side,
                            price: matched_resting_bid_order.price,
                            buyer_limit_price: matched_resting_bid_order.price,
                            quantity: matched_quantity,
                            maker_fully_filled: true,
                            taker_fully_filled: remaining_order.remaining == 0,
                        };

                        assert_eq!(
                            Self::pop_front_order_id_at_price(&mut self.bids, best_bid_price),
                            Some(matched_resting_bid_order_id)
                        );
                        assert!(
                            self.open_orders
                                .remove(&matched_resting_bid_order_id)
                                .is_some()
                        );
                        fills.push(trade);
                    };
                }
                let resting_order: Option<Order> = if remaining_order.remaining > 0 {
                    self.insert(remaining_order.clone());
                    Some(remaining_order)
                } else {
                    None
                };
                Ok(MatchResult {
                    resting_order,
                    fills,
                })
            }
        }
    }

    // just inserts order into orderbook based on the side
    fn insert(&mut self, order: Order) {
        match order.side {
            Side::Bid => {
                let order_id = order.id;
                self.bids
                    .entry(order.price)
                    .and_modify(|price_level| {
                        price_level.push_back(order_id).unwrap();
                    })
                    .or_insert_with(|| PriceLevel::new(order_id));

                self.open_orders.insert(order.id, order)
            }
            Side::Ask => {
                let order_id = order.id;
                self.asks
                    .entry(order.price)
                    .and_modify(|price_level| {
                        price_level.push_back(order_id).unwrap();
                    })
                    .or_insert_with(|| PriceLevel::new(order_id));

                self.open_orders.insert(order.id, order)
            }
        };
    }
}

fn main() {
    println!("starting matching engine....");

    let symbol = "BTC".to_string();
    let mut exchange = Exchange::new();
    exchange.insert_symbol(symbol.clone());

    exchange.accounts_manager.create_account(1);
    exchange
        .accounts_manager
        .increase_position_quantity_for_symbol(1, symbol.clone(), 3)
        .unwrap();
    exchange.accounts_manager.create_account(2);
    exchange
        .accounts_manager
        .increase_position_quantity_for_symbol(2, symbol.clone(), 4)
        .unwrap();
    exchange.accounts_manager.create_account(3);
    exchange
        .accounts_manager
        .increase_account_balance(3, 1000)
        .unwrap();

    println!(
        "balances before matching -> {:?}",
        exchange.accounts_manager
    );

    let ask_order_1 = Order {
        id: 1,
        user_id: 1,
        symbol: symbol.clone(),
        side: Side::Ask,
        price: 100,
        remaining: 3,
    };

    println!(
        "balance check ask_order_1 -> {:?}",
        exchange
            .accounts_manager
            .is_enough_balance_for_order(&ask_order_1)
    );

    let ask_order_2 = Order {
        id: 2,
        user_id: 2,
        symbol: symbol.clone(),
        side: Side::Ask,
        price: 100,
        remaining: 4,
    };

    println!(
        "balance check ask_order_2 -> {:?}",
        exchange
            .accounts_manager
            .is_enough_balance_for_order(&ask_order_2)
    );

    let bid_order = Order {
        id: 3,
        user_id: 3,
        symbol: symbol.clone(),
        side: Side::Bid,
        price: 100,
        remaining: 10,
    };

    println!(
        "balance check bid_order -> {:?}",
        exchange
            .accounts_manager
            .is_enough_balance_for_order(&bid_order)
    );

    exchange.submit_limit_order(ask_order_1).unwrap();

    exchange.submit_limit_order(ask_order_2).unwrap();

    println!(
        "book before bid -> {:?}",
        exchange.orderbooks.get(&symbol).unwrap()
    );

    let result = exchange.submit_limit_order(bid_order).unwrap();
    println!("result -> {:?}", result);
    println!(
        "book after bid -> {:?}",
        exchange.orderbooks.get(&symbol).unwrap()
    );
    println!("balances after matching -> {:?}", exchange.accounts_manager);

    let cancelled = exchange.cancel_order(3, symbol.clone(), 3).unwrap();
    println!("cancelled -> {:?}", cancelled);
}
