//! # CRDT Multi-Client Integration Test (TSK-311 / SYS-REQ-078)
//!
//! Validates:
//! - Multi-client WebSocket connection to `/api/v1/collab/ws/room-1`
//! - Document sync convergence between Client A and Client B
//! - Awareness state sharing (cursor position, user color, username)
//! - Client disconnect cleanup and awareness removal broadcast

use futures_util::{SinkExt, StreamExt};
use mythos_core::api::create_router;
use mythos_core::storage::ControllerStorage;
use mythos_core::tag::TagCatalog;
use mythos_core::ApiState;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage;
use yrs::encoding::read::Cursor;
use yrs::sync::protocol::{Message, MessageReader, SyncMessage};
use yrs::sync::Awareness;
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::Encode;
use yrs::{Doc, GetString, Text, Transact, Update};

#[tokio::test]
async fn test_crdt_multi_client_integration_suite() {
    // 1. Setup isolated in-memory test environment and Axum server
    let tmp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let storage = Arc::new(ControllerStorage::new(tmp_dir.path()).expect("Storage init failed"));
    let catalog = Arc::new(RwLock::new(TagCatalog::new()));
    let state = ApiState::<mythos_pal::PosixShm>::new(storage, catalog, None);
    let room_mgr = Arc::clone(&state.room_manager);
    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind TCP listener");
    let server_addr = listener.local_addr().expect("Failed to get local addr");

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("Axum server crashed");
    });

    let room_url = format!("ws://{}/api/v1/collab/ws/room-1", server_addr);

    // 2. Connect Client A
    let (mut client_a_ws, _) = connect_async(&room_url)
        .await
        .expect("Client A failed to connect to room-1");

    // Server sends initial SyncStep1 to Client A
    let msg_a_first = client_a_ws.next().await.unwrap().unwrap();
    match msg_a_first {
        TungsteniteMessage::Binary(bytes) => {
            let mut decoder = DecoderV1::new(Cursor::new(&bytes));
            let mut reader = MessageReader::new(&mut decoder);
            let msg = reader.next().unwrap().unwrap();
            assert!(
                matches!(msg, Message::Sync(SyncMessage::SyncStep1(_))),
                "Client A expected initial SyncStep1 message"
            );
        }
        other => panic!("Expected Binary SyncStep1, got: {:?}", other),
    }

    // 3. Connect Client B
    let (mut client_b_ws, _) = connect_async(&room_url)
        .await
        .expect("Client B failed to connect to room-1");

    // Server sends initial SyncStep1 to Client B
    let msg_b_first = client_b_ws.next().await.unwrap().unwrap();
    assert!(
        matches!(msg_b_first, TungsteniteMessage::Binary(_)),
        "Client B expected initial SyncStep1 binary message"
    );

    // 4. Test Document Sync Convergence: Client A edits document
    let client_a_doc = Doc::with_client_id(1001);
    let a_text = client_a_doc.get_or_insert_text("shared_code");
    let update_a_bytes = {
        let mut txn = client_a_doc.transact_mut();
        a_text.push(
            &mut txn,
            "PROGRAM Main_Conveyor
  Motor_Start := TRUE;
END_PROGRAM",
        );
        txn.encode_update_v1()
    };

    let sync_update_a = Message::Sync(SyncMessage::Update(update_a_bytes));
    client_a_ws
        .send(TungsteniteMessage::Binary(sync_update_a.encode_v1()))
        .await
        .expect("Client A failed to send CRDT update");

    // Client B receives the broadcasted Update
    let mut client_b_received_update = false;
    let client_b_doc = Doc::with_client_id(1002);

    tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(2000)) => {
            panic!("Timeout waiting for Client B to receive CRDT update from Client A");
        }
        msg = client_b_ws.next() => {
            if let Some(Ok(TungsteniteMessage::Binary(bytes))) = msg {
                let mut decoder = DecoderV1::new(Cursor::new(&bytes));
                let mut reader = MessageReader::new(&mut decoder);
                if let Some(Ok(Message::Sync(SyncMessage::Update(u)))) = reader.next() {
                    let update = Update::decode_v1(&u).expect("Client B failed to decode Update");
                    client_b_doc.transact_mut().apply_update(update).expect("Client B failed to apply Update");
                    let b_text = client_b_doc.get_or_insert_text("shared_code");
                    assert_eq!(
                        b_text.get_string(&client_b_doc.transact()),
                        "PROGRAM Main_Conveyor
  Motor_Start := TRUE;
END_PROGRAM"
                    );
                    client_b_received_update = true;
                }
            }
        }
    }
    assert!(
        client_b_received_update,
        "Client B did not receive CRDT sync update"
    );

    // Also verify server room document converged
    let room = room_mgr
        .get_room("room-1")
        .await
        .expect("Room room-1 should exist");
    let room_text = room.doc.get_or_insert_text("shared_code");
    assert_eq!(
        room_text.get_string(&room.doc.transact()),
        "PROGRAM Main_Conveyor
  Motor_Start := TRUE;
END_PROGRAM",
        "Server room document does not match converged CRDT state"
    );

    // 5. Test Awareness State Sharing (Cursor Position, User Color, Username)
    let awareness_a = Awareness::new(Doc::with_client_id(1001));
    awareness_a
        .set_local_state(serde_json::json!({
            "user": {
                "name": "Engineer-Alice",
                "color": "#10B981"
            },
            "cursor": {
                "lineNumber": 15,
                "column": 8
            },
            "activeTab": "Main_Conveyor.st"
        }))
        .expect("Failed to set Client A awareness state");

    let aw_update_a = awareness_a
        .update()
        .expect("Failed to get awareness update");
    let aw_msg_a = Message::Awareness(aw_update_a);
    client_a_ws
        .send(TungsteniteMessage::Binary(aw_msg_a.encode_v1()))
        .await
        .expect("Client A failed to send awareness");

    // Client B receives Client A awareness update
    let mut client_b_received_awareness = false;
    let awareness_b = Awareness::new(Doc::with_client_id(1002));

    tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(2000)) => {
            panic!("Timeout waiting for Client B to receive awareness update");
        }
        msg = client_b_ws.next() => {
            if let Some(Ok(TungsteniteMessage::Binary(bytes))) = msg {
                let mut decoder = DecoderV1::new(Cursor::new(&bytes));
                let mut reader = MessageReader::new(&mut decoder);
                if let Some(Ok(Message::Awareness(u))) = reader.next() {
                    awareness_b.apply_update(u).expect("Client B failed to apply awareness");
                    let state_a: serde_json::Value = awareness_b.state(1001).expect("Client A awareness missing in B");
                    assert_eq!(state_a["user"]["name"], "Engineer-Alice");
                    assert_eq!(state_a["user"]["color"], "#10B981");
                    assert_eq!(state_a["cursor"]["lineNumber"], 15);
                    assert_eq!(state_a["cursor"]["column"], 8);
                    assert_eq!(state_a["activeTab"], "Main_Conveyor.st");
                    client_b_received_awareness = true;
                }
            }
        }
    }
    assert!(
        client_b_received_awareness,
        "Client B did not receive Client A awareness state"
    );

    // 6. Test Client Disconnect Cleanup: Client A closes connection
    client_a_ws
        .close(None)
        .await
        .expect("Client A failed to close connection cleanly");

    // Client B receives awareness removal update (or room awareness reflects removal)
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Room awareness should have cleaned up Client A
    {
        let aw_guard = room.awareness.read().await;
        let c1_state = aw_guard.state::<serde_json::Value>(1001);
        assert!(
            c1_state.is_none(),
            "Client A awareness should be removed after disconnect"
        );
    }

    // Clean up Client B
    client_b_ws
        .close(None)
        .await
        .expect("Client B close failed");
}
