use std::collections::{BTreeMap, HashMap, VecDeque};

type OrderId = u64;
type Price = u64;
type Quantity = u64;
type UserId = u64;
type Symbol = String;

#[derive(Debug, Clone, Copy)]
enum Side {
    Bid,
    Ask,
}
#[derive(Debug, Clone)]
struct Order {
    id: OrderId,
    user_id: UserId,
    side: Side,
    price: Price,
    remaining: Quantity,
}

#[derive(Debug)]
struct Trade {
    maker_order_id: OrderId,
    taker_order_id: OrderId,
    price: Price,
    quantity: Quantity,
}

#[derive(Debug)]
struct MatchResult {
    resting_order: Option<Order>,
    fills: Vec<Trade>,
}

#[derive(Debug)]
struct BalancesManager {
    balances: HashMap<UserId, UserAccount>,
}

#[derive(Debug)]
struct UserAccount {
    balance: Price,
    locked: Price,
    positions: HashMap<Symbol, UserPosition>,
}

#[derive(Debug)]
struct UserPosition {
    quantity: Quantity,
    locked: Quantity,
}

impl BalancesManager {
    fn new() -> Self {
        BalancesManager {
            balances: HashMap::new(),
        }
    }

    fn insert_account(&mut self, user_id: UserId, account: UserAccount) {
        self.balances.insert(user_id, account);
    }
}

impl UserAccount {
    fn new(balance: Price, positions: HashMap<Symbol, UserPosition>) -> Self {
        UserAccount {
            balance,
            locked: 0,
            positions,
        }
    }
}

impl UserPosition {
    fn new(quantity: Quantity) -> Self {
        UserPosition {
            quantity,
            locked: 0,
        }
    }
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

    // create an empty orderbook
    let mut orderbook = Orderbook::new();
    let mut balances = BalancesManager::new();
    let symbol = "STOCK".to_string();

    let mut user_1_positions = HashMap::new();
    user_1_positions.insert(symbol.clone(), UserPosition::new(3));
    balances.insert_account(1, UserAccount::new(0, user_1_positions));

    let mut user_2_positions = HashMap::new();
    user_2_positions.insert(symbol.clone(), UserPosition::new(4));
    balances.insert_account(2, UserAccount::new(0, user_2_positions));

    let user_3_positions = HashMap::new();
    balances.insert_account(3, UserAccount::new(2_000, user_3_positions));

    println!("balances before matching -> {:?}", balances);

    let ask_order_1 = Order {
        id: 1,
        user_id: 1,
        side: Side::Ask,
        price: 100,
        remaining: 3,
    };

    let ask_order_2 = Order {
        id: 2,
        user_id: 2,
        side: Side::Ask,
        price: 101,
        remaining: 4,
    };

    let bid_order = Order {
        id: 3,
        user_id: 3,
        side: Side::Bid,
        price: 101,
        remaining: 10,
    };

    orderbook.handle_limit_order(ask_order_1);
    orderbook.handle_limit_order(ask_order_2);

    println!("book before bid -> {:?}", orderbook);

    let result = orderbook.handle_limit_order(bid_order);
    println!("result -> {:?}", result);
    println!("book after bid -> {:?}", orderbook);
}
