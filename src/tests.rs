use super::accounts::{AccountsError, AccountsManager, UserAccount, UserPosition};
use super::{MatchResult, Order, OrderId, Orderbook, Price, Quantity, Side, Trade, UserId};

fn btc_order(id: OrderId, user_id: UserId, side: Side, price: Price, remaining: Quantity) -> Order {
    Order {
        id,
        user_id,
        side,
        symbol: "BTC".to_string(),
        price,
        remaining,
    }
}

fn account(accounts: &AccountsManager, user_id: UserId) -> &UserAccount {
    accounts.accounts.get(&user_id).unwrap()
}

fn btc_position(account: &UserAccount) -> &UserPosition {
    account.positions.get("BTC").unwrap()
}

fn settle_all(accounts: &mut AccountsManager, result: &MatchResult) {
    for fill in &result.fills {
        accounts.settle_fill(fill).unwrap();
    }
}

fn seed_seller(
    accounts: &mut AccountsManager,
    book: &mut Orderbook,
    user_id: UserId,
    order_id: OrderId,
    price: Price,
    quantity: Quantity,
) {
    accounts.create_account(user_id);
    accounts
        .increase_position_quantity_for_symbol(user_id, "BTC".to_string(), quantity)
        .unwrap();

    let ask = btc_order(order_id, user_id, Side::Ask, price, quantity);
    accounts.reserve_user_position_for_order(&ask).unwrap();

    let result = book.handle_limit_order(ask).unwrap();
    assert!(result.fills.is_empty());
}

fn seed_buyer(accounts: &mut AccountsManager, user_id: UserId, balance: Price) {
    accounts.create_account(user_id);
    accounts.increase_account_balance(user_id, balance).unwrap();
}

#[test]
fn same_price_partial_fill_keeps_remaining_bid_reserved_without_refund() {
    let mut book = Orderbook::new();
    let mut accounts = AccountsManager::new();

    seed_seller(&mut accounts, &mut book, 1, 1, 100, 7);
    seed_buyer(&mut accounts, 2, 1_000);

    let bid = btc_order(2, 2, Side::Bid, 100, 10);
    accounts.reserve_user_balance_for_order(&bid).unwrap();

    let result = book.handle_limit_order(bid).unwrap();
    settle_all(&mut accounts, &result);

    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.resting_order.as_ref().unwrap().remaining, 3);

    let buyer = account(&accounts, 2);
    let reservation = buyer.reservations.get(&2).unwrap();
    assert_eq!(buyer.balance, 0);
    assert_eq!(reservation.price, 100);
    assert_eq!(reservation.quantity, 3);
    assert_eq!(btc_position(buyer).quantity, 7);

    let seller = account(&accounts, 1);
    assert_eq!(seller.balance, 700);
    assert!(btc_position(seller).reservations.is_empty());
}

#[test]
fn price_improved_partial_bid_fill_refunds_filled_spread_and_keeps_rest_reserved() {
    let mut book = Orderbook::new();
    let mut accounts = AccountsManager::new();

    seed_seller(&mut accounts, &mut book, 1, 1, 90, 7);
    seed_buyer(&mut accounts, 2, 1_000);

    let bid = btc_order(2, 2, Side::Bid, 100, 10);
    accounts.reserve_user_balance_for_order(&bid).unwrap();

    let result = book.handle_limit_order(bid).unwrap();
    settle_all(&mut accounts, &result);

    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.fills[0].price, 90);
    assert_eq!(result.fills[0].quantity, 7);
    assert_eq!(result.resting_order.as_ref().unwrap().remaining, 3);

    let buyer = account(&accounts, 2);
    let reservation = buyer.reservations.get(&2).unwrap();
    assert_eq!(buyer.balance, 70);
    assert_eq!(reservation.price, 100);
    assert_eq!(reservation.quantity, 3);
    assert_eq!(btc_position(buyer).quantity, 7);

    let seller = account(&accounts, 1);
    assert_eq!(seller.balance, 630);
    assert!(btc_position(seller).reservations.is_empty());
}

#[test]
fn price_improved_full_bid_fill_refunds_and_removes_bid_reservation() {
    let mut book = Orderbook::new();
    let mut accounts = AccountsManager::new();

    seed_seller(&mut accounts, &mut book, 1, 1, 90, 10);
    seed_buyer(&mut accounts, 2, 1_000);

    let bid = btc_order(2, 2, Side::Bid, 100, 10);
    accounts.reserve_user_balance_for_order(&bid).unwrap();

    let result = book.handle_limit_order(bid).unwrap();
    settle_all(&mut accounts, &result);

    assert_eq!(result.fills.len(), 1);
    assert!(result.resting_order.is_none());

    let buyer = account(&accounts, 2);
    assert_eq!(buyer.balance, 100);
    assert!(!buyer.reservations.contains_key(&2));
    assert_eq!(btc_position(buyer).quantity, 10);

    let seller = account(&accounts, 1);
    assert_eq!(seller.balance, 900);
    assert!(btc_position(seller).reservations.is_empty());
}

#[test]
fn price_improved_bid_across_multiple_ask_levels_refunds_each_fill() {
    let mut book = Orderbook::new();
    let mut accounts = AccountsManager::new();

    seed_seller(&mut accounts, &mut book, 1, 1, 90, 3);
    seed_seller(&mut accounts, &mut book, 2, 2, 95, 4);
    seed_buyer(&mut accounts, 3, 1_000);

    let bid = btc_order(3, 3, Side::Bid, 100, 10);
    accounts.reserve_user_balance_for_order(&bid).unwrap();

    let result = book.handle_limit_order(bid).unwrap();
    settle_all(&mut accounts, &result);

    assert_eq!(result.fills.len(), 2);
    assert_eq!(result.fills[0].price, 90);
    assert_eq!(result.fills[0].quantity, 3);
    assert_eq!(result.fills[1].price, 95);
    assert_eq!(result.fills[1].quantity, 4);
    assert_eq!(result.resting_order.as_ref().unwrap().remaining, 3);

    let buyer = account(&accounts, 3);
    let reservation = buyer.reservations.get(&3).unwrap();
    assert_eq!(buyer.balance, 50);
    assert_eq!(reservation.price, 100);
    assert_eq!(reservation.quantity, 3);
    assert_eq!(btc_position(buyer).quantity, 7);

    assert_eq!(account(&accounts, 1).balance, 270);
    assert_eq!(account(&accounts, 2).balance, 380);
}

#[test]
fn incoming_ask_consumes_resting_bid_maker_reservation() {
    let mut book = Orderbook::new();
    let mut accounts = AccountsManager::new();

    seed_buyer(&mut accounts, 1, 1_000);
    let resting_bid = btc_order(1, 1, Side::Bid, 100, 10);
    accounts
        .reserve_user_balance_for_order(&resting_bid)
        .unwrap();

    let resting_bid_result = book.handle_limit_order(resting_bid).unwrap();
    assert!(resting_bid_result.fills.is_empty());

    accounts.create_account(2);
    accounts
        .increase_position_quantity_for_symbol(2, "BTC".to_string(), 7)
        .unwrap();

    let ask = btc_order(2, 2, Side::Ask, 90, 7);
    accounts.reserve_user_position_for_order(&ask).unwrap();

    let result = book.handle_limit_order(ask).unwrap();
    settle_all(&mut accounts, &result);

    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.fills[0].price, 100);
    assert_eq!(result.fills[0].quantity, 7);
    assert!(result.resting_order.is_none());

    let buyer = account(&accounts, 1);
    let reservation = buyer.reservations.get(&1).unwrap();
    assert_eq!(buyer.balance, 0);
    assert_eq!(reservation.price, 100);
    assert_eq!(reservation.quantity, 3);
    assert_eq!(btc_position(buyer).quantity, 7);

    let seller = account(&accounts, 2);
    assert_eq!(seller.balance, 700);
    assert_eq!(btc_position(seller).quantity, 0);
    assert!(btc_position(seller).reservations.is_empty());
}

#[test]
fn missing_bid_reservation_returns_error_during_settlement() {
    let mut accounts = AccountsManager::new();
    accounts.create_account(1);
    accounts.create_account(2);

    let fill = Trade {
        maker_order_id: 1,
        taker_order_id: 2,
        maker_user_id: 1,
        taker_user_id: 2,
        symbol: "BTC".to_string(),
        taker_side: Side::Bid,
        price: 90,
        quantity: 1,
    };

    assert!(matches!(
        accounts.settle_fill(&fill),
        Err(AccountsError::UnknwonReservation)
    ));
}
