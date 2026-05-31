use super::super::{INITIAL_SYMBOLS, Order, OrderId, Price, Quantity, Side, Symbol, Trade, UserId};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct AccountsManager {
    pub accounts: HashMap<UserId, UserAccount>,
}

#[derive(Debug)]
pub struct UserAccount {
    pub balance: Price,
    pub balance_reservations: HashSet<OrderId>,
    pub positions: HashMap<Symbol, UserPosition>,
}

#[derive(Debug)]
pub struct UserPosition {
    pub quantity: Quantity,
    pub position_reservations: HashSet<OrderId>,
}

impl UserAccount {
    fn new() -> Self {
        let mut positions: HashMap<Symbol, UserPosition> = HashMap::new();

        for symbol in INITIAL_SYMBOLS {
            positions
                .entry(symbol.to_string())
                .or_insert_with(UserPosition::new);
        }

        UserAccount {
            balance: 0,
            balance_reservations: HashSet::new(),
            positions,
        }
    }

    fn reserve_balance(&mut self, order: &Order) {
        let amount = order.price * order.remaining;
        self.balance -= amount;
        self.balance_reservations.insert(order.id);
    }

    fn increase_balance(&mut self, amount: Price) {
        self.balance += amount;
    }

    fn has_balance_reservation(&self, order_id: OrderId) -> Result<(), AccountsError> {
        if !self.balance_reservations.contains(&order_id) {
            return Err(AccountsError::UnknwonReservation);
        };
        Ok(())
    }

    fn get_position(&self, symbol: Symbol) -> Result<&UserPosition, AccountsError> {
        let Some(position) = self.positions.get(&symbol) else {
            return Err(AccountsError::InvalidSymbol);
        };
        Ok(position)
    }

    fn get_position_mut(&mut self, symbol: Symbol) -> Result<&mut UserPosition, AccountsError> {
        let Some(position) = self.positions.get_mut(&symbol) else {
            return Err(AccountsError::InvalidSymbol);
        };
        Ok(position)
    }

    fn consume_reserved_balance(
        &mut self,
        reserved_price: Price,
        match_price: Price,
        match_quantity: Quantity,
        order_id: OrderId,
        order_fully_filled: bool,
    ) -> Result<(), AccountsError> {
        self.has_balance_reservation(order_id)?;

        let amount_reserved_for_quantity = match_quantity * reserved_price;
        let actual_amount_spent = match_quantity * match_price;
        let refund_amount = amount_reserved_for_quantity - actual_amount_spent;
        self.balance += refund_amount;

        if order_fully_filled {
            self.balance_reservations.remove(&order_id);
        }

        Ok(())
    }

    fn consume_reserved_position_quantity(
        &mut self,
        symbol: Symbol,
        order_id: OrderId,
        match_price: Price,
        match_quantity: Quantity,
        order_fully_filled: bool,
    ) -> Result<(), AccountsError> {
        let position = self.get_position_mut(symbol)?;
        position.has_position_reservation(order_id)?;

        if order_fully_filled {
            position.remove_reservation(order_id);
        }

        self.balance += match_price * match_quantity;
        Ok(())
    }
}

impl UserPosition {
    fn new() -> Self {
        UserPosition {
            quantity: 0,
            position_reservations: HashSet::new(),
        }
    }
    fn remove_reservation(&mut self, order_id: OrderId) {
        self.position_reservations.remove(&order_id);
    }

    fn reserve_position(&mut self, order: &Order) {
        self.quantity -= order.remaining;
        self.position_reservations.insert(order.id);
    }

    fn has_position_reservation(&self, order_id: OrderId) -> Result<(), AccountsError> {
        if !self.position_reservations.contains(&order_id) {
            return Err(AccountsError::UnknwonReservation);
        };
        Ok(())
    }

    fn increase_position_quantity(&mut self, quantity: Quantity) {
        self.quantity += quantity;
    }
}

#[derive(Debug)]
pub enum AccountsError {
    InvalidPrice,
    InvalidQuantity,
    UnknownUser,
    InsufficientBalance,
    InsufficientPositionQuantity,
    InvalidSymbol,
    UnknwonReservation,
}

impl AccountsManager {
    pub fn new() -> Self {
        AccountsManager {
            accounts: HashMap::new(),
        }
    }

    pub fn create_account(&mut self, user_id: UserId) {
        let user_account = UserAccount::new();
        self.accounts.insert(user_id, user_account);
    }

    pub fn increase_account_balance(
        &mut self,
        user_id: UserId,
        amount: Price,
    ) -> Result<(), AccountsError> {
        let user_account = self.get_user_account_mut(user_id)?;
        user_account.increase_balance(amount);
        Ok(())
    }

    pub fn consume_reserved_balance(
        &mut self,
        user_id: UserId,
        order_id: OrderId,
        reserved_price: Price,
        match_price: Price,
        match_quantity: Quantity,
        order_fully_filled: bool,
    ) -> Result<(), AccountsError> {
        let user_account = self.get_user_account_mut(user_id)?;
        user_account.consume_reserved_balance(
            reserved_price,
            match_price,
            match_quantity,
            order_id,
            order_fully_filled,
        )?;
        Ok(())
    }

    pub fn increase_position_quantity_for_symbol(
        &mut self,
        user_id: UserId,
        symbol: Symbol,
        quantity: Quantity,
    ) -> Result<(), AccountsError> {
        let position = self.get_position_for_symbol_mut(user_id, symbol)?;
        position.increase_position_quantity(quantity);
        Ok(())
    }

    pub fn consume_reserved_position_quantity_for_symbol(
        &mut self,
        user_id: UserId,
        symbol: Symbol,
        order_id: OrderId,
        match_price: Price,
        match_quantity: Quantity,
        order_fully_filled: bool,
    ) -> Result<(), AccountsError> {
        let account = self.get_user_account_mut(user_id)?;
        account.consume_reserved_position_quantity(
            symbol,
            order_id,
            match_price,
            match_quantity,
            order_fully_filled,
        )?;
        Ok(())
    }

    pub fn get_user_account(&self, user_id: UserId) -> Result<&UserAccount, AccountsError> {
        let Some(user_account) = self.accounts.get(&user_id) else {
            return Err(AccountsError::UnknownUser);
        };
        Ok(user_account)
    }

    pub fn get_user_account_mut(
        &mut self,
        user_id: UserId,
    ) -> Result<&mut UserAccount, AccountsError> {
        let Some(user_account) = self.accounts.get_mut(&user_id) else {
            return Err(AccountsError::UnknownUser);
        };
        Ok(user_account)
    }

    pub fn get_balance(&self, user_id: UserId) -> Result<Price, AccountsError> {
        self.get_user_account(user_id)
            .map(|account| account.balance)
    }

    pub fn get_position_for_symbol(
        &self,
        user_id: UserId,
        symbol: Symbol,
    ) -> Result<&UserPosition, AccountsError> {
        let account = self.get_user_account(user_id)?;
        account.get_position(symbol)
    }

    pub fn get_position_for_symbol_mut(
        &mut self,
        user_id: UserId,
        symbol: Symbol,
    ) -> Result<&mut UserPosition, AccountsError> {
        let account = self.get_user_account_mut(user_id)?;
        account.get_position_mut(symbol)
    }

    pub fn get_position_quantity_for_symbol(
        &self,
        user_id: UserId,
        symbol: Symbol,
    ) -> Result<Quantity, AccountsError> {
        let position = self.get_position_for_symbol(user_id, symbol)?;
        Ok(position.quantity)
    }

    pub fn is_enough_balance_for_order(&self, order: &Order) -> Result<(), AccountsError> {
        match order.side {
            Side::Bid => {
                let balance = self.get_balance(order.user_id)?;
                let min_required_balance = order.remaining * order.price;

                if balance >= min_required_balance {
                    Ok(())
                } else {
                    Err(AccountsError::InsufficientBalance)
                }
            }
            Side::Ask => {
                let position_quantity =
                    self.get_position_quantity_for_symbol(order.user_id, order.symbol.clone())?;
                if order.remaining <= position_quantity {
                    Ok(())
                } else {
                    Err(AccountsError::InsufficientPositionQuantity)
                }
            }
        }
    }

    pub fn reserve_user_balance_for_order(&mut self, order: &Order) -> Result<(), AccountsError> {
        let user_account = self.get_user_account_mut(order.user_id)?;
        user_account.reserve_balance(order);
        Ok(())
    }

    pub fn reserve_user_position_for_order(&mut self, order: &Order) -> Result<(), AccountsError> {
        let position = self.get_position_for_symbol_mut(order.user_id, order.symbol.clone())?;
        position.reserve_position(order);
        Ok(())
    }

    pub fn release_reserved_order(&mut self, order: &Order) -> Result<(), AccountsError> {
        match order.side {
            Side::Bid => {
                let user_account = self.get_user_account_mut(order.user_id)?;
                user_account.has_balance_reservation(order.id)?;
                user_account.balance += order.price * order.remaining;
                user_account.balance_reservations.remove(&order.id);
            }
            Side::Ask => {
                let position =
                    self.get_position_for_symbol_mut(order.user_id, order.symbol.clone())?;
                position.has_position_reservation(order.id)?;
                position.quantity += order.remaining;
                position.remove_reservation(order.id);
            }
        }

        Ok(())
    }

    pub fn settle_fill(&mut self, trade: &Trade) -> Result<(), AccountsError> {
        match trade.taker_side {
            Side::Bid => {
                self.consume_reserved_balance(
                    trade.taker_user_id,
                    trade.taker_order_id,
                    trade.buyer_limit_price,
                    trade.price,
                    trade.quantity,
                    trade.taker_fully_filled,
                )?;
                self.increase_position_quantity_for_symbol(
                    trade.taker_user_id,
                    trade.symbol.clone(),
                    trade.quantity,
                )?;
                self.consume_reserved_position_quantity_for_symbol(
                    trade.maker_user_id,
                    trade.symbol.clone(),
                    trade.maker_order_id,
                    trade.price,
                    trade.quantity,
                    trade.maker_fully_filled,
                )?;
            }
            Side::Ask => {
                self.consume_reserved_position_quantity_for_symbol(
                    trade.taker_user_id,
                    trade.symbol.clone(),
                    trade.taker_order_id,
                    trade.price,
                    trade.quantity,
                    trade.taker_fully_filled,
                )?;
                self.consume_reserved_balance(
                    trade.maker_user_id,
                    trade.maker_order_id,
                    trade.buyer_limit_price,
                    trade.price,
                    trade.quantity,
                    trade.maker_fully_filled,
                )?;
                self.increase_position_quantity_for_symbol(
                    trade.maker_user_id,
                    trade.symbol.clone(),
                    trade.quantity,
                )?;
            }
        }

        Ok(())
    }
}
