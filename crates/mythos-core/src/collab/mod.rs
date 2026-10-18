//! # CRDT Collaboration Engine (Yrs)
//!
//! Provides multi-client real-time CRDT synchronization and awareness tracking
//! for industrial logic diagrams, structured text, and topology schematics.
//!
//! Implements the standard Yjs / Yrs binary sync protocol:
//! - `SyncStep1`: Handshake exchange of local `StateVector`.
//! - `SyncStep2`: Missing document diffs calculation and transmission.
//! - `Update`: Continuous incremental block updates replicated across peers.
//! - `Awareness`: Ephemeral state broadcast (cursor positions, presence, selection)
//!   with automatic disconnect cleanup.

use axum::extract::ws::{Message as WsMessage, WebSocket};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};
use yrs::encoding::read::Cursor;
use yrs::sync::protocol::{Message, MessageReader, SyncMessage};
use yrs::sync::Awareness;
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::Encode;
use yrs::{Doc, ReadTxn, Transact, Update};

/// Collaboration room wrapping a shared CRDT `Doc`, ephemeral `Awareness` tracker,
/// and a broadcast channel distributing deltas to connected peer sessions.
pub struct CollabRoom {
    /// Shared Yrs CRDT document containing the canonical room state.
    pub doc: Doc,
    /// Ephemeral awareness tracker for presence and cursor tracking.
    pub awareness: Arc<RwLock<Awareness>>,
    /// Multi-producer multi-consumer channel broadcasting binary deltas to peer connections.
    broadcast_tx: broadcast::Sender<(u64, Arc<Vec<u8>>)>,
    /// Monotonically increasing connection ID counter.
    next_conn_id: AtomicU64,
}

impl Default for CollabRoom {
    fn default() -> Self {
        Self::new()
    }
}

impl CollabRoom {
    /// Create a new, empty collaboration room.
    pub fn new() -> Self {
        let doc = Doc::new();
        let awareness = Arc::new(RwLock::new(Awareness::new(doc.clone())));
        let (broadcast_tx, _) = broadcast::channel(1024);
        Self {
            doc,
            awareness,
            broadcast_tx,
            next_conn_id: AtomicU64::new(1),
        }
    }

    /// Allocate a unique connection identifier for this room.
    pub fn allocate_conn_id(&self) -> u64 {
        self.next_conn_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Broadcast a binary protocol message to all peers in the room except `sender_id`.
    pub fn broadcast(&self, sender_id: u64, message_bytes: Vec<u8>) {
        let _ = self.broadcast_tx.send((sender_id, Arc::new(message_bytes)));
    }

    /// Subscribe to room broadcast messages.
    pub fn subscribe(&self) -> broadcast::Receiver<(u64, Arc<Vec<u8>>)> {
        self.broadcast_tx.subscribe()
    }
}

/// Thread-safe registry managing active collaboration rooms by identifier.
#[derive(Clone, Default)]
pub struct RoomManager {
    rooms: Arc<RwLock<HashMap<String, Arc<CollabRoom>>>>,
}

impl RoomManager {
    /// Create a new, empty `RoomManager`.
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Retrieve an existing collaboration room or create a new one if it does not exist.
    pub async fn get_or_create_room(&self, room_id: &str) -> Arc<CollabRoom> {
        let mut rooms = self.rooms.write().await;
        rooms
            .entry(room_id.to_string())
            .or_insert_with(|| Arc::new(CollabRoom::new()))
            .clone()
    }

    /// Get a reference to an active collaboration room if it exists.
    pub async fn get_room(&self, room_id: &str) -> Option<Arc<CollabRoom>> {
        let rooms = self.rooms.read().await;
        rooms.get(room_id).cloned()
    }

    /// Remove a room by identifier.
    pub async fn remove_room(&self, room_id: &str) -> Option<Arc<CollabRoom>> {
        let mut rooms = self.rooms.write().await;
        rooms.remove(room_id)
    }

    /// Count total active rooms currently managed.
    pub async fn room_count(&self) -> usize {
        let rooms = self.rooms.read().await;
        rooms.len()
    }
}

/// Axum WebSocket protocol handler managing the bidirectional Yjs / Yrs sync lifecycle.
///
/// Features:
/// - Handshake initiation: Sends `SyncStep1` with local document `StateVector`.
/// - Sync handling: Responds to remote `SyncStep1` with calculated `SyncStep2`.
/// - Mutation propagation: Applies incoming `SyncStep2` or `Update` to local doc and broadcasts to peers.
/// - Awareness coordination: Applies presence updates, answers awareness queries, and replicates changes.
/// - Health & Keep-alive: 30-second ping/pong heartbeat.
/// - Graceful disconnection: Automatically cleans and broadcasts awareness removals when connection terminates.
pub async fn handle_collab_ws(mut ws: WebSocket, room: Arc<CollabRoom>) {
    let conn_id = room.allocate_conn_id();
    let mut rx = room.subscribe();
    let mut tracked_clients: HashSet<u64> = HashSet::new();

    debug!(conn_id, "Collab client connected");

    // 1. Initial Handshake: Send local SyncStep1 (state_vector) to client
    let initial_sv = {
        let txn = room.doc.transact();
        txn.state_vector()
    };
    let step1_msg = Message::Sync(SyncMessage::SyncStep1(initial_sv));
    if let Err(e) = ws.send(WsMessage::Binary(step1_msg.encode_v1())).await {
        warn!(conn_id, "Failed to send initial SyncStep1: {}", e);
        return;
    }

    // 2. Send current awareness state if any clients exist
    let initial_awareness = {
        let aw = room.awareness.read().await;
        aw.update().ok()
    };
    if let Some(update) = initial_awareness {
        if !update.clients.is_empty() {
            let aw_msg = Message::Awareness(update);
            if let Err(e) = ws.send(WsMessage::Binary(aw_msg.encode_v1())).await {
                warn!(conn_id, "Failed to send initial awareness update: {}", e);
                return;
            }
        }
    }

    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
    // Consume the initial immediate tick
    ping_interval.tick().await;

    // 3. Bidirectional Event Loop
    'event_loop: loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                if let Err(e) = ws.send(WsMessage::Ping(vec![])).await {
                    debug!(conn_id, "Ping failed, closing connection: {}", e);
                    break 'event_loop;
                }
            }

            bcast_res = rx.recv() => {
                match bcast_res {
                    Ok((sender_id, data)) => {
                        if sender_id != conn_id {
                            if let Err(e) = ws.send(WsMessage::Binary((*data).clone())).await {
                                debug!(conn_id, "Failed to forward broadcast delta: {}", e);
                                break 'event_loop;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(conn_id, skipped, "Peer broadcast channel lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!(conn_id, "Broadcast channel closed");
                        break 'event_loop;
                    }
                }
            }

            incoming = ws.recv() => {
                match incoming {
                    Some(Ok(WsMessage::Binary(bytes))) => {
                        let mut decoder = DecoderV1::new(Cursor::new(&bytes));
                        let mut reader = MessageReader::new(&mut decoder);

                        for msg_result in &mut reader {
                            match msg_result {
                                Ok(Message::Sync(SyncMessage::SyncStep1(remote_sv))) => {
                                    // Reply with SyncStep2 containing missing updates
                                    let update_diff = {
                                        let txn = room.doc.transact();
                                        txn.encode_state_as_update_v1(&remote_sv)
                                    };
                                    let reply = Message::Sync(SyncMessage::SyncStep2(update_diff));
                                    if let Err(e) = ws.send(WsMessage::Binary(reply.encode_v1())).await {
                                        warn!(conn_id, "Failed to send SyncStep2 reply: {}", e);
                                        break 'event_loop;
                                    }
                                }
                                Ok(Message::Sync(SyncMessage::SyncStep2(update_bytes))) => {
                                    match Update::decode_v1(&update_bytes) {
                                        Ok(update) => {
                                            {
                                                let mut txn = room.doc.transact_mut();
                                                if let Err(e) = txn.apply_update(update) {
                                                    error!(conn_id, "Failed to apply SyncStep2 update: {}", e);
                                                }
                                            }
                                            // Broadcast update to all other connected peers
                                            let bcast_msg = Message::Sync(SyncMessage::Update(update_bytes));
                                            room.broadcast(conn_id, bcast_msg.encode_v1());
                                        }
                                        Err(e) => {
                                            warn!(conn_id, "Malformed SyncStep2 payload: {:?}", e);
                                        }
                                    }
                                }
                                Ok(Message::Sync(SyncMessage::Update(update_bytes))) => {
                                    match Update::decode_v1(&update_bytes) {
                                        Ok(update) => {
                                            {
                                                let mut txn = room.doc.transact_mut();
                                                if let Err(e) = txn.apply_update(update) {
                                                    error!(conn_id, "Failed to apply Update: {}", e);
                                                }
                                            }
                                            // Broadcast update to all other connected peers
                                            let bcast_msg = Message::Sync(SyncMessage::Update(update_bytes));
                                            room.broadcast(conn_id, bcast_msg.encode_v1());
                                        }
                                        Err(e) => {
                                            warn!(conn_id, "Malformed Update payload: {:?}", e);
                                        }
                                    }
                                }
                                Ok(Message::AwarenessQuery) => {
                                    let aw = room.awareness.read().await;
                                    if let Ok(update) = aw.update() {
                                        let reply = Message::Awareness(update);
                                        if let Err(e) = ws.send(WsMessage::Binary(reply.encode_v1())).await {
                                            warn!(conn_id, "Failed to send Awareness query reply: {}", e);
                                            break 'event_loop;
                                        }
                                    }
                                }
                                Ok(Message::Awareness(awareness_update)) => {
                                    for &client_id in awareness_update.clients.keys() {
                                        tracked_clients.insert(client_id);
                                    }
                                    {
                                        let aw = room.awareness.write().await;
                                        if let Err(e) = aw.apply_update(awareness_update.clone()) {
                                            warn!(conn_id, "Failed to apply awareness update: {}", e);
                                        }
                                    }
                                    // Broadcast awareness update to other peers
                                    let bcast_msg = Message::Awareness(awareness_update);
                                    room.broadcast(conn_id, bcast_msg.encode_v1());
                                }
                                Ok(Message::Auth(_)) => {
                                    debug!(conn_id, "Auth message received (unimplemented/permitted)");
                                }
                                Ok(Message::Custom(tag, data)) => {
                                    debug!(conn_id, tag, len = data.len(), "Custom protocol message received");
                                }
                                Err(e) => {
                                    warn!(conn_id, "Failed to decode protocol message: {:?}", e);
                                }
                            }
                        }
                    }
                    Some(Ok(WsMessage::Ping(payload))) => {
                        if let Err(e) = ws.send(WsMessage::Pong(payload)).await {
                            debug!(conn_id, "Failed to respond to Ping: {}", e);
                            break 'event_loop;
                        }
                    }
                    Some(Ok(WsMessage::Pong(_))) => {
                        // Keep-alive acknowledged
                    }
                    Some(Ok(WsMessage::Close(_))) => {
                        debug!(conn_id, "Client sent Close frame");
                        break 'event_loop;
                    }
                    Some(Ok(WsMessage::Text(txt))) => {
                        debug!(conn_id, len = txt.len(), "Ignoring unexpected text frame");
                    }
                    Some(Err(e)) => {
                        debug!(conn_id, "WebSocket receive error: {}", e);
                        break 'event_loop;
                    }
                    None => {
                        debug!(conn_id, "WebSocket client closed stream");
                        break 'event_loop;
                    }
                }
            }
        }
    }

    // 4. Graceful Disconnection: Clear awareness states and broadcast removal to peers
    if !tracked_clients.is_empty() {
        debug!(
            conn_id,
            count = tracked_clients.len(),
            "Cleaning up awareness for disconnected client(s)"
        );
        let client_ids: Vec<u64> = tracked_clients.into_iter().collect();
        let aw = room.awareness.write().await;
        for &cid in &client_ids {
            aw.remove_state(cid);
        }
        if let Ok(removal_update) = aw.update_with_clients(client_ids) {
            let bcast_msg = Message::Awareness(removal_update);
            room.broadcast(conn_id, bcast_msg.encode_v1());
        }
    }

    info!(conn_id, "Collab client session terminated");
}

#[cfg(test)]
mod tests;
