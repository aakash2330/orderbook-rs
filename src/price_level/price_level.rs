use std::collections::HashMap;

use slab::Slab;

use crate::OrderId;

type PriceLevelNodeId = usize;

#[derive(Debug)]
pub(crate) enum PriceLevelError {
    CannotRemoveLastOrder,
    DuplicateOrder,
    UnknownOrder,
    UnknownPriceLevelNode,
    CorruptLinks,
}

#[derive(Debug)]
pub struct PriceLevel {
    head: PriceLevelNodeId,
    tail: PriceLevelNodeId,
    nodes: Slab<PriceLevelNode>,
    order_node_mapping: HashMap<OrderId, PriceLevelNodeId>,
}

impl PriceLevel {
    pub(crate) fn new(order_id: OrderId) -> PriceLevel {
        let mut order_node_mapping: HashMap<OrderId, PriceLevelNodeId> = HashMap::new();
        let mut nodes: Slab<PriceLevelNode> = Slab::new();
        let price_level_node = PriceLevelNode::new(order_id, None, None);
        let node_id = nodes.insert(price_level_node);
        order_node_mapping.insert(order_id, node_id);

        PriceLevel {
            head: node_id,
            tail: node_id,
            nodes: nodes,
            order_node_mapping,
        }
    }
    pub(crate) fn delete(&mut self, order_id: OrderId) -> Result<OrderId, PriceLevelError> {
        if self.nodes.len() == 1 {
            return Err(PriceLevelError::CannotRemoveLastOrder);
        }

        // find node Id from mapping
        let price_level_node_id = self
            .order_node_mapping
            .get(&order_id)
            .copied()
            .ok_or(PriceLevelError::UnknownOrder)?;
        let Some(removed_price_level_node) = self.nodes.try_remove(price_level_node_id) else {
            return Err(PriceLevelError::UnknownPriceLevelNode);
        };

        //update the prev,next,head,tail
        if let Some(prev_node_id) = removed_price_level_node.prev {
            let prev_node = self
                .nodes
                .get_mut(prev_node_id)
                .ok_or(PriceLevelError::CorruptLinks)?;
            prev_node.next = removed_price_level_node.next;
        } else {
            self.head = removed_price_level_node
                .next
                .ok_or(PriceLevelError::CorruptLinks)?;
        }
        if let Some(next_node_id) = removed_price_level_node.next {
            let next_node = self
                .nodes
                .get_mut(next_node_id)
                .ok_or(PriceLevelError::CorruptLinks)?;
            next_node.prev = removed_price_level_node.prev;
        } else {
            self.tail = removed_price_level_node
                .prev
                .ok_or(PriceLevelError::CorruptLinks)?;
        }

        //remove mapping
        self.remove_mapping(order_id)?;

        //TODO:remove price level if it was the last one -> this should be orderbook level function
        Ok(removed_price_level_node.order_id)
    }

    pub(crate) fn push_back(&mut self, order_id: OrderId) -> Result<(), PriceLevelError> {
        if self.order_node_mapping.contains_key(&order_id) {
            return Err(PriceLevelError::DuplicateOrder);
        }

        let price_level_node = PriceLevelNode::new(order_id, Some(self.tail), None);
        let price_level_node_id = self.nodes.insert(price_level_node);

        // update second last node's next
        let second_last_entry = self
            .nodes
            .get_mut(self.tail)
            .ok_or(PriceLevelError::CorruptLinks)?;
        second_last_entry.next = Some(price_level_node_id);

        // udpate self.tail
        self.tail = price_level_node_id;

        // insert_mapping;
        self.insert_mapping(order_id, price_level_node_id);
        Ok(())
    }

    fn insert_mapping(&mut self, order_id: OrderId, price_level_node_id: PriceLevelNodeId) {
        self.order_node_mapping
            .insert(order_id, price_level_node_id);
    }

    fn remove_mapping(&mut self, order_id: OrderId) -> Result<(), PriceLevelError> {
        self.order_node_mapping
            .remove(&order_id)
            .map(|_| ())
            .ok_or(PriceLevelError::UnknownOrder)
    }

    pub(crate) fn pop_front(&mut self) -> Result<OrderId, PriceLevelError> {
        if self.nodes.len() == 1 {
            return Err(PriceLevelError::CannotRemoveLastOrder);
        }

        let Some(removed_price_level_node) = self.nodes.try_remove(self.head) else {
            return Err(PriceLevelError::UnknownPriceLevelNode);
        };
        let new_head = removed_price_level_node
            .next
            .ok_or(PriceLevelError::CorruptLinks)?;
        let next_node = self
            .nodes
            .get_mut(new_head)
            .ok_or(PriceLevelError::CorruptLinks)?;
        // because if there's only one node , it should've been handled by orderbook
        next_node.prev = None;
        self.head = new_head;
        self.remove_mapping(removed_price_level_node.order_id)?;
        Ok(removed_price_level_node.order_id)
    }

    pub(crate) fn front(&self) -> Result<OrderId, PriceLevelError> {
        let Some(price_level_node) = self.nodes.get(self.head) else {
            return Err(PriceLevelError::CorruptLinks);
        };
        Ok(price_level_node.order_id)
    }

    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }
}

#[derive(Debug)]
struct PriceLevelNode {
    order_id: OrderId,
    prev: Option<PriceLevelNodeId>,
    next: Option<PriceLevelNodeId>,
}

impl PriceLevelNode {
    pub fn new(
        order_id: OrderId,
        prev: Option<PriceLevelNodeId>,
        next: Option<PriceLevelNodeId>,
    ) -> PriceLevelNode {
        PriceLevelNode {
            order_id,
            prev,
            next,
        }
    }
}
