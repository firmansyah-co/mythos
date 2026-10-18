//! Tests for CRDT Collaboration Engine (Yrs).

use super::*;
use crate::{ApiState, ControllerStorage};
use std::time::Duration;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{GetString, Text};

#[tokio::test]
async fn test_room_manager_lifecycle() {
    let manager = RoomManager::new();
    assert_eq!(manager.room_count().await, 0);

    let room1 = manager.get_or_create_room("project-alpha").await;
    assert_eq!(manager.room_count().await, 1);

    // Retrieving same room returns the identical Arc reference
    let room1_again = manager.get_or_create_room("project-alpha").await;
    assert_eq!(manager.room_count().await, 1);
    assert!(Arc::ptr_eq(&room1, &room1_again));

    // Create a second room
    let _room2 = manager.get_or_create_room("project-beta").await;
    assert_eq!(manager.room_count().await, 2);

    // Get existing room
    let fetched = manager.get_room("project-alpha").await;
    assert!(fetched.is_some());

    // Remove room
    let removed = manager.remove_room("project-alpha").await;
    assert!(removed.is_some());
    assert_eq!(manager.room_count().await, 1);
    assert!(manager.get_room("project-alpha").await.is_none());
}

#[tokio::test]
async fn test_collab_room_broadcast() {
    let room = CollabRoom::new();
    let id1 = room.allocate_conn_id();
    let id2 = room.allocate_conn_id();
    assert_ne!(id1, id2);

    let mut rx1 = room.subscribe();
    let mut rx2 = room.subscribe();

    // Broadcast from id1
    room.broadcast(id1, vec![1, 2, 3]);

    // id2 receives message, sender is id1
    let (sender, data) = rx2.recv().await.unwrap();
    assert_eq!(sender, id1);
    assert_eq!(*data, vec![1, 2, 3]);

    // id1 also receives raw broadcast event from channel, but in handle_collab_ws it is filtered
    let (sender_self, data_self) = rx1.recv().await.unwrap();
    assert_eq!(sender_self, id1);
    assert_eq!(*data_self, vec![1, 2, 3]);
}

#[tokio::test]
async fn test_two_client_crdt_doc_synchronization() {
    let room = Arc::new(CollabRoom::new());

    // Initialize room document with initial logic text
    {
        let text = room.doc.get_or_insert_text("logic_st");
        let mut txn = room.doc.transact_mut();
        text.push(&mut txn, "VAR_GLOBAL\n  RunCmd: BOOL;\nEND_VAR\n");
    }

    // Client 1 connects: requests full sync by sending empty StateVector
    let client1_doc = Doc::with_client_id(101);
    let c1_sv = client1_doc.transact().state_vector();

    // Server calculates diff for Client 1
    let diff_for_c1 = room.doc.transact().encode_state_as_update_v1(&c1_sv);
    let step2_c1 = Message::Sync(SyncMessage::SyncStep2(diff_for_c1));

    // Client 1 decodes and applies
    let encoded_step2 = step2_c1.encode_v1();
    let mut decoder = DecoderV1::new(Cursor::new(&encoded_step2));
    let mut reader = MessageReader::new(&mut decoder);
    if let Some(Ok(Message::Sync(SyncMessage::SyncStep2(bytes)))) = reader.next() {
        let update = Update::decode_v1(&bytes).unwrap();
        client1_doc.transact_mut().apply_update(update).unwrap();
    } else {
        panic!("Expected SyncStep2");
    }

    // Verify Client 1 has identical text
    {
        let c1_text = client1_doc.get_or_insert_text("logic_st");
        assert_eq!(
            c1_text.get_string(&client1_doc.transact()),
            "VAR_GLOBAL\n  RunCmd: BOOL;\nEND_VAR\n"
        );
    }

    // Client 2 connects and syncs
    let client2_doc = Doc::with_client_id(102);
    let c2_sv = client2_doc.transact().state_vector();
    let diff_for_c2 = room.doc.transact().encode_state_as_update_v1(&c2_sv);
    let update_c2 = Update::decode_v1(&diff_for_c2).unwrap();
    client2_doc.transact_mut().apply_update(update_c2).unwrap();

    // Client 1 makes an edit: appends code
    let c1_update_bytes = {
        let c1_text = client1_doc.get_or_insert_text("logic_st");
        let mut txn = client1_doc.transact_mut();
        c1_text.push(&mut txn, "PROGRAM Main\n  RunCmd := TRUE;\nEND_PROGRAM");
        txn.encode_update_v1()
    };

    // Client 1 sends Update to server
    let update_msg = Message::Sync(SyncMessage::Update(c1_update_bytes.clone()));
    let encoded_update = update_msg.encode_v1();

    // Server receives and applies to room doc
    let mut s_decoder = DecoderV1::new(Cursor::new(&encoded_update));
    let mut s_reader = MessageReader::new(&mut s_decoder);
    if let Some(Ok(Message::Sync(SyncMessage::Update(bytes)))) = s_reader.next() {
        let update = Update::decode_v1(&bytes).unwrap();
        room.doc.transact_mut().apply_update(update).unwrap();
        // Server broadcasts to Client 2
        let c2_apply = Update::decode_v1(&bytes).unwrap();
        client2_doc.transact_mut().apply_update(c2_apply).unwrap();
    }

    // Both clients and server have converged!
    let expected =
        "VAR_GLOBAL\n  RunCmd: BOOL;\nEND_VAR\nPROGRAM Main\n  RunCmd := TRUE;\nEND_PROGRAM";
    let server_text = room
        .doc
        .get_or_insert_text("logic_st")
        .get_string(&room.doc.transact());
    let c1_text = client1_doc
        .get_or_insert_text("logic_st")
        .get_string(&client1_doc.transact());
    let c2_text = client2_doc
        .get_or_insert_text("logic_st")
        .get_string(&client2_doc.transact());

    assert_eq!(server_text, expected);
    assert_eq!(c1_text, expected);
    assert_eq!(c2_text, expected);
}

#[tokio::test]
async fn test_two_client_awareness_and_graceful_disconnect() {
    let room = Arc::new(CollabRoom::new());

    // Client 1 registers presence (cursor at line 10, col 5)
    let c1_client_id: u64 = 201;
    let c1_doc = Doc::with_client_id(c1_client_id);
    let c1_awareness = Awareness::new(c1_doc);
    c1_awareness
        .set_local_state(serde_json::json!({
            "user": "Adrian",
            "cursor": {"line": 10, "col": 5},
            "color": "#00ff88"
        }))
        .unwrap();

    let c1_update = c1_awareness.update().unwrap();
    let c1_msg = Message::Awareness(c1_update);
    let c1_encoded = c1_msg.encode_v1();

    // Server receives Client 1 awareness
    let mut decoder = DecoderV1::new(Cursor::new(&c1_encoded));
    let mut reader = MessageReader::new(&mut decoder);
    if let Some(Ok(Message::Awareness(u))) = reader.next() {
        let aw = room.awareness.write().await;
        aw.apply_update(u).unwrap();
    }

    // Client 2 connects and queries awareness
    let aw = room.awareness.read().await;
    let room_aw_update = aw.update().unwrap();
    assert!(room_aw_update.clients.contains_key(&c1_client_id));
    let client1_state: serde_json::Value = aw.state(c1_client_id).unwrap();
    assert_eq!(client1_state["user"], "Adrian");
    assert_eq!(client1_state["cursor"]["line"], 10);
    drop(aw);

    // Client 2 registers presence
    let c2_client_id: u64 = 202;
    let c2_doc = Doc::with_client_id(c2_client_id);
    let c2_awareness = Awareness::new(c2_doc);
    c2_awareness
        .set_local_state(serde_json::json!({
            "user": "Neo",
            "cursor": {"line": 42, "col": 1},
            "color": "#3b82f6"
        }))
        .unwrap();

    let c2_update = c2_awareness.update().unwrap();
    {
        let aw = room.awareness.write().await;
        aw.apply_update(c2_update).unwrap();
        assert_eq!(aw.iter().count(), 2);
    }

    // Graceful disconnect: Client 1 leaves
    {
        let aw = room.awareness.write().await;
        aw.remove_state(c1_client_id);
        assert!(aw.state::<serde_json::Value>(c1_client_id).is_none());

        // Broadcast removal update
        let removal = aw.update_with_clients(vec![c1_client_id]).unwrap();
        assert_eq!(removal.clients[&c1_client_id].json.as_ref(), "null");

        // Client 2 receives removal update
        c2_awareness.apply_update(removal).unwrap();
        assert!(c2_awareness
            .state::<serde_json::Value>(c1_client_id)
            .is_none());
    }

    // Client 2 is still active in room
    let aw = room.awareness.read().await;
    let client2_state: serde_json::Value = aw.state(c2_client_id).unwrap();
    assert_eq!(client2_state["user"], "Neo");
}

#[tokio::test]
async fn test_axum_collab_ws_route_integration() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage;

    // Build real ApiState and router
    let tmp_dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(ControllerStorage::new(tmp_dir.path()).unwrap());
    let catalog = Arc::new(tokio::sync::RwLock::new(crate::tag::TagCatalog::new()));
    let state = ApiState::<mythos_pal::PosixShm>::new(storage, catalog, None);
    let room_mgr = Arc::clone(&state.room_manager);
    let app = crate::api::create_router(state);

    // Bind local TCP listener
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();

    // Spawn server task
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let ws_url = format!("ws://{}/api/v1/collab/ws/integration-room", local_addr);

    // Connect Client 1
    let (mut client1_ws, _) = connect_async(&ws_url)
        .await
        .expect("Client 1 connect failed");

    // Client 1 receives initial SyncStep1 from server
    let first_msg = client1_ws.next().await.unwrap().unwrap();
    match first_msg {
        TungsteniteMessage::Binary(bytes) => {
            let mut decoder = DecoderV1::new(Cursor::new(&bytes));
            let mut reader = MessageReader::new(&mut decoder);
            let msg = reader.next().unwrap().unwrap();
            assert!(matches!(msg, Message::Sync(SyncMessage::SyncStep1(_))));
        }
        other => panic!("Expected Binary SyncStep1, got: {:?}", other),
    }

    // Connect Client 2
    let (mut client2_ws, _) = connect_async(&ws_url)
        .await
        .expect("Client 2 connect failed");

    // Client 2 receives initial SyncStep1
    let c2_first = client2_ws.next().await.unwrap().unwrap();
    assert!(matches!(c2_first, TungsteniteMessage::Binary(_)));

    // Client 1 sends an Update to the server
    let local_doc = Doc::with_client_id(777);
    let text = local_doc.get_or_insert_text("code");
    let update_bytes = {
        let mut txn = local_doc.transact_mut();
        text.push(&mut txn, "FUNCTION_BLOCK FB_Motor");
        txn.encode_update_v1()
    };
    let update_msg = Message::Sync(SyncMessage::Update(update_bytes));
    client1_ws
        .send(TungsteniteMessage::Binary(update_msg.encode_v1()))
        .await
        .unwrap();

    // Client 2 should receive the broadcasted Update!
    let mut received_update = false;
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(1500)) => {}
        msg = client2_ws.next() => {
            if let Some(Ok(TungsteniteMessage::Binary(bytes))) = msg {
                let mut decoder = DecoderV1::new(Cursor::new(&bytes));
                let mut reader = MessageReader::new(&mut decoder);
                if let Some(Ok(Message::Sync(SyncMessage::Update(u)))) = reader.next() {
                    let client2_doc = Doc::new();
                    let update = Update::decode_v1(&u).unwrap();
                    client2_doc.transact_mut().apply_update(update).unwrap();
                    let c2_text = client2_doc.get_or_insert_text("code");
                    assert_eq!(c2_text.get_string(&client2_doc.transact()), "FUNCTION_BLOCK FB_Motor");
                    received_update = true;
                }
            }
        }
    }
    assert!(
        received_update,
        "Client 2 did not receive the broadcasted update in time"
    );

    // Also verify the server room doc has the update
    let room = room_mgr.get_room("integration-room").await.unwrap();
    let room_text = room.doc.get_or_insert_text("code");
    assert_eq!(
        room_text.get_string(&room.doc.transact()),
        "FUNCTION_BLOCK FB_Motor"
    );

    // Close Client 1
    client1_ws.close(None).await.unwrap();
    // Close Client 2
    client2_ws.close(None).await.unwrap();
}
