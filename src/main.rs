mod accounts;
use accounts::AccountsManager;
use std::collections::{BTreeMap, HashMap, VecDeque};

#[cfg(test)]
mod tests;

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
    quantity: Quantity,
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

// priority is first the price and then the oldest order
#[derive(Debug)]
struct Orderbook {
    bids: BTreeMap<Price, VecDeque<Order>>,
    asks: BTreeMap<Price, VecDeque<Order>>,
}

#[derive(Debug)]
struct Exchange {
    orderbooks: HashMap<Symbol, Orderbook>,
}

impl Exchange {
    fn new() -> Self {
        let mut orderbooks: HashMap<Symbol, Orderbook> = HashMap::new();

        for symbol in INITIAL_SYMBOLS {
            orderbooks.insert(symbol.to_string(), Orderbook::new());
        }

        Exchange { orderbooks }
    }

    fn insert_symbol(&mut self, symbol: Symbol) {
        self.orderbooks.insert(symbol, Orderbook::new());
    }
}

impl Orderbook {
    fn new() -> Self {
        let bids: BTreeMap<Price, VecDeque<Order>> = BTreeMap::new();
        let asks: BTreeMap<Price, VecDeque<Order>> = BTreeMap::new();
        Orderbook { bids, asks }
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

    fn get_resting_order(_order_id: String) -> Option<Order> {
        None
    }

    fn insert_order(side: &mut BTreeMap<Price, VecDeque<Order>>, order: Order) {
        side.entry(order.price).or_default().push_back(order);
    }

    fn remove_ask_price_if_empty(&mut self, price: Price) {
        if self.asks.get(&price).unwrap().is_empty() {
            self.asks.remove(&price);
        };
    }

    fn remove_bid_price_if_empty(&mut self, price: Price) {
        if self.bids.get(&price).unwrap().is_empty() {
            self.bids.remove(&price);
        };
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
                    let matched_resting_ask_order = self
                        .asks
                        .get_mut(&best_ask_price)
                        .unwrap()
                        .front_mut()
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
                            quantity: matched_quantity,
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
                            quantity: matched_quantity,
                        };

                        assert!(
                            self.asks
                                .get_mut(&best_ask_price)
                                .unwrap()
                                .pop_front()
                                .is_some()
                        );
                        self.remove_ask_price_if_empty(best_ask_price);
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
                    let matched_resting_bid_order = self
                        .bids
                        .get_mut(&best_bid_price)
                        .unwrap()
                        .front_mut()
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
                            quantity: matched_quantity,
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
                            quantity: matched_quantity,
                        };

                        assert!(
                            self.bids
                                .get_mut(&best_bid_price)
                                .unwrap()
                                .pop_front()
                                .is_some()
                        );
                        self.remove_bid_price_if_empty(best_bid_price);
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
            Side::Bid => Self::insert_order(&mut self.bids, order),
            Side::Ask => Self::insert_order(&mut self.asks, order),
        };
    }
}

fn main() {
    println!("starting matching engine....");

    let symbol = "BTC".to_string();
    let mut exchange = Exchange::new();
    exchange.insert_symbol(symbol.clone());
    let orderbook = exchange.orderbooks.get_mut(&symbol).unwrap();
    let mut accounts = AccountsManager::new();

    accounts.create_account(1);
    accounts
        .increase_position_quantity_for_symbol(1, symbol.clone(), 3)
        .unwrap();
    accounts.create_account(2);
    accounts
        .increase_position_quantity_for_symbol(2, symbol.clone(), 4)
        .unwrap();
    accounts.create_account(3);
    accounts.increase_account_balance(3, 1000).unwrap();

    println!("balances before matching -> {:?}", accounts);

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
        accounts.is_enough_balance_for_order(&ask_order_1)
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
        accounts.is_enough_balance_for_order(&ask_order_2)
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
        accounts.is_enough_balance_for_order(&bid_order)
    );

    accounts
        .reserve_user_position_for_order(&ask_order_1)
        .unwrap();
    let result1 = orderbook.handle_limit_order(ask_order_1).unwrap();
    for fill in &result1.fills {
        accounts.settle_fill(fill).unwrap();
    }

    accounts
        .reserve_user_position_for_order(&ask_order_2)
        .unwrap();
    let result2 = orderbook.handle_limit_order(ask_order_2).unwrap();
    for fill in &result2.fills {
        accounts.settle_fill(fill).unwrap();
    }

    println!("book before bid -> {:?}", orderbook);

    accounts.reserve_user_balance_for_order(&bid_order).unwrap();
    let result = orderbook.handle_limit_order(bid_order).unwrap();
    for fill in &result.fills {
        accounts.settle_fill(fill).unwrap();
    }
    println!("result -> {:?}", result);
    println!("book after bid -> {:?}", orderbook);
    println!("balances after matching -> {:?}", accounts);
}
