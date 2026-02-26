//! Execution Engine Crate (OMS).
//!
//! Manages Order State, Fills, Timeouts, and Idempotency.
//! Handles Bracket Orders and Trailing Stops.

use core_types::{Order, OrderRequest, OrderStatus, SymbolId, FillData, Side, OrderType, StateSyncData, PositionData};
use std::collections::{HashMap, HashSet};

pub struct OrderManagementSystem {
    // Maps internal order_id to Order
    pub orders: HashMap<u64, Order>,
    // Maps idempotency_key (client_order_id) to internal order_id
    pub idempotency_cache: HashMap<String, u64>,
    // Maps broker_order_id to internal order_id
    pub broker_map: HashMap<String, u64>,

    next_order_id: u64,
}

impl OrderManagementSystem {
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
            idempotency_cache: HashMap::new(),
            broker_map: HashMap::new(),
            next_order_id: 1,
        }
    }

    /// Handles a new order request.
    /// Returns (Internal Order ID, Optional Bracket Orders to send)
    /// If idempotency key exists, returns existing Order ID.
    pub fn place_order(&mut self, request: OrderRequest, timestamp_us: u64) -> Result<u64, &'static str> {
        if let Some(&existing_id) = self.idempotency_cache.get(&request.idempotency_key) {
            return Ok(existing_id);
        }

        let order_id = self.next_order_id;
        self.next_order_id += 1;

        let order = Order {
            order_id,
            client_order_id: request.idempotency_key.clone(),
            broker_order_id: None,
            symbol_id: request.symbol_id,
            side: request.side,
            qty: request.qty,
            filled_qty: 0,
            avg_fill_price: 0.0,
            order_type: request.order_type,
            limit_price: request.limit_price,
            stop_price: request.stop_price,
            status: OrderStatus::Pending,
            created_at: timestamp_us,
            updated_at: timestamp_us,
        };

        self.orders.insert(order_id, order);
        self.idempotency_cache.insert(request.idempotency_key, order_id);

        Ok(order_id)
    }

    pub fn handle_ack(&mut self, internal_id: u64, broker_id: String) {
        if let Some(order) = self.orders.get_mut(&internal_id) {
            order.broker_order_id = Some(broker_id.clone());
            // If it was Pending, move to Live
            if order.status == OrderStatus::Pending {
                order.status = OrderStatus::Live;
            }
            self.broker_map.insert(broker_id, internal_id);
        }
    }

    pub fn handle_fill(&mut self, fill: FillData, timestamp_us: u64) {
        // Assuming fill.order_id corresponds to internal order_id
        if let Some(order) = self.orders.get_mut(&fill.order_id) {
            order.updated_at = timestamp_us;

            // Update average price
            let total_cost = (order.filled_qty as f64 * order.avg_fill_price) + (fill.size as f64 * fill.price);
            order.filled_qty += fill.size;
            if order.filled_qty > 0 {
                order.avg_fill_price = total_cost / order.filled_qty as f64;
            }

            if order.filled_qty >= order.qty {
                order.status = OrderStatus::Filled;
            } else {
                 // Partial fill, ensure status implies Open/Live
                 if order.status != OrderStatus::Live {
                     order.status = OrderStatus::Live;
                 }
            }
        }
    }

    pub fn cancel_order(&mut self, order_id: u64, timestamp_us: u64) {
         if let Some(order) = self.orders.get_mut(&order_id) {
             order.status = OrderStatus::Cancelled;
             order.updated_at = timestamp_us;
         }
    }

    pub fn check_timeouts(&mut self, now_us: u64, timeout_us: u64) -> Vec<u64> {
        let mut timed_out = Vec::new();
        for (id, order) in &self.orders {
            if order.status == OrderStatus::Pending || order.status == OrderStatus::Live {
                if now_us > order.created_at && (now_us - order.created_at) > timeout_us {
                    timed_out.push(*id);
                }
            }
        }
        timed_out
    }

    /// Reconciles local state with broker truth.
    /// Returns a list of stale internal Order IDs that were cancelled locally.
    pub fn reconcile_state(&mut self, sync_data: StateSyncData) -> Vec<u64> {
        let mut stale_orders = Vec::new();
        let mut broker_order_ids = HashSet::new();

        // 1. Process Open Orders from Broker
        for broker_order in sync_data.open_orders {
            if let Some(id) = broker_order.broker_order_id.clone() {
                broker_order_ids.insert(id.clone());

                // Check if we know this order
                if let Some(&internal_id) = self.broker_map.get(&id) {
                    // Update existing
                    if let Some(local_order) = self.orders.get_mut(&internal_id) {
                        local_order.status = broker_order.status;
                        local_order.filled_qty = broker_order.filled_qty;
                        local_order.avg_fill_price = broker_order.avg_fill_price;
                    }
                } else if let Some(&internal_id) = self.idempotency_cache.get(&broker_order.client_order_id) {
                     // Match by client ID (e.g. we sent it but didn't get ack yet)
                     if let Some(local_order) = self.orders.get_mut(&internal_id) {
                        local_order.broker_order_id = Some(id.clone());
                        self.broker_map.insert(id, internal_id);
                        local_order.status = broker_order.status;
                        local_order.filled_qty = broker_order.filled_qty;
                        local_order.avg_fill_price = broker_order.avg_fill_price;
                     }
                } else {
                    // Unknown order from broker (e.g. placed manually or before restart)
                    // Import it
                    // We need to generate a local ID
                    let new_id = self.next_order_id;
                    self.next_order_id += 1;

                    let mut new_order = broker_order.clone();
                    new_order.order_id = new_id;

                    self.orders.insert(new_id, new_order);
                    self.broker_map.insert(id, new_id);
                    // We don't have client_order_id in cache if it wasn't placed by this run,
                    // but we can add it if needed.
                    if !broker_order.client_order_id.is_empty() {
                        self.idempotency_cache.insert(broker_order.client_order_id.clone(), new_id);
                    }
                }
            }
        }

        // 2. Identify Stale Local Orders
        // Any local order that is Live/Pending but NOT in broker_order_ids is stale/lost
        for (id, order) in &mut self.orders {
            if (order.status == OrderStatus::Live || order.status == OrderStatus::Pending) {
                // If it has a broker ID, it should have been in the sync list
                if let Some(bid) = &order.broker_order_id {
                    if !broker_order_ids.contains(bid) {
                        // Broker doesn't have it open -> It's done (Filled or Cancelled) or Lost.
                        // Safe bet: Mark Cancelled/Unknown
                        order.status = OrderStatus::Cancelled;
                        stale_orders.push(*id);
                    }
                } else {
                    // No broker ID yet (Pending Ack).
                    // If sync happens, and we don't see it, maybe it failed?
                    // Or maybe it's *just* being sent.
                    // Strict Reconnect: assume if not in sync, it's not live.
                    order.status = OrderStatus::Cancelled;
                    stale_orders.push(*id);
                }
            }
        }

        stale_orders
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_types::{TimeInForce, OrderType, Side};

    #[test]
    fn test_place_order_idempotency() {
        let mut oms = OrderManagementSystem::new();
        let request = OrderRequest {
            symbol_id: SymbolId(1),
            side: Side::Bid,
            qty: 100,
            order_type: OrderType::Limit,
            limit_price: Some(10.0),
            stop_price: None,
            tif: TimeInForce::GTC,
            idempotency_key: "key1".to_string(),
            take_profit_price: None,
            stop_loss_price: None,
        };

        let id1 = oms.place_order(request.clone(), 1000).unwrap();
        let id2 = oms.place_order(request, 2000).unwrap();

        assert_eq!(id1, id2);
        assert_eq!(oms.orders.len(), 1);
        assert_eq!(oms.orders[&id1].created_at, 1000);
    }

    #[test]
    fn test_handle_fill() {
        let mut oms = OrderManagementSystem::new();
        let request = OrderRequest {
            symbol_id: SymbolId(1),
            side: Side::Bid,
            qty: 100,
            order_type: OrderType::Limit,
            limit_price: Some(10.0),
            stop_price: None,
            tif: TimeInForce::GTC,
            idempotency_key: "key1".to_string(),
            take_profit_price: None,
            stop_loss_price: None,
        };

        let id = oms.place_order(request, 1000).unwrap();

        let fill = FillData {
            order_id: id,
            price: 10.0,
            size: 50,
            side: Side::Bid,
            liquidity: 0,
        };
        oms.handle_fill(fill, 1100);

        let order = &oms.orders[&id];
        assert_eq!(order.filled_qty, 50);
        assert_eq!(order.status, OrderStatus::Live);

        let fill2 = FillData {
            order_id: id,
            price: 10.0,
            size: 50,
            side: Side::Bid,
            liquidity: 0,
        };
        oms.handle_fill(fill2, 1200);

        let order = &oms.orders[&id];
        assert_eq!(order.filled_qty, 100);
        assert_eq!(order.status, OrderStatus::Filled);
    }

    #[test]
    fn test_timeouts() {
        let mut oms = OrderManagementSystem::new();
        let request = OrderRequest {
            symbol_id: SymbolId(1),
            side: Side::Bid,
            qty: 100,
            order_type: OrderType::Limit,
            limit_price: Some(10.0),
            stop_price: None,
            tif: TimeInForce::GTC,
            idempotency_key: "key1".to_string(),
            take_profit_price: None,
            stop_loss_price: None,
        };

        let id = oms.place_order(request, 1000).unwrap();

        // Timeout 500us
        // Check at 1200 (200 elapsed) -> Not timeout
        assert!(oms.check_timeouts(1200, 500).is_empty());

        // Check at 1600 (600 elapsed) -> Timeout
        let timed_out = oms.check_timeouts(1600, 500);
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0], id);
    }
}
