//! Integration tests for PROFINET DCP Dual-Role Engine and mDNS Fallback Discovery.
//!
//! Enforces compliance with SYS-REQ-040, SYS-REQ-041, SYS-REQ-042, SYS-REQ-043,
//! and tasks TSK-201, TSK-202, TSK-203.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use mythos_core::discovery::{
    DcpClient, DcpDiscoverer, DcpServer, DcpServerConfig, DiscoveryContext, DiscoveryEngine,
    DiscoveryMode, DnsPacket, EthernetFrame, LoopbackRawTransport, MacAddress, MdnsDiscoverer,
    MdnsDiscoveryClient, MdnsResponder, MdnsService, MdnsTransport, SimulatedMdnsTransport,
    DEVICE_ROLE_IO_CONTROLLER, DEVICE_ROLE_IO_DEVICE, ETHERTYPE_PROFINET, SERVICE_TYPE_MYTHOS,
    SERVICE_TYPE_OPCUA, VENDOR_ID_FIRMANSYAH,
};

#[test]
fn test_tsk201_raw_socket_ethernet_frame_bounds_and_structure() {
    let src = MacAddress::new([0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E]);
    let dst = MacAddress::BROADCAST;
    let payload = vec![0xFE, 0xFD, 0x05, 0x00, 0xAA, 0xBB, 0xCC, 0xDD];

    let frame = EthernetFrame::new(dst, src, ETHERTYPE_PROFINET, payload.clone());
    let raw = frame.to_bytes(true);

    // Minimum Ethernet frame size is 60 bytes (excluding 4-byte FCS)
    assert!(raw.len() >= 60);

    let parsed = EthernetFrame::parse(&raw).expect("parse valid raw frame");
    assert_eq!(parsed.destination, dst);
    assert_eq!(parsed.source, src);
    assert_eq!(parsed.ethertype, ETHERTYPE_PROFINET);
    // Payload contains original data (padded at end to reach 46 bytes payload minimum)
    assert_eq!(&parsed.payload[..payload.len()], &payload[..]);

    // Truncated buffer check (no panics)
    assert!(EthernetFrame::parse(&[0x00, 0x11, 0x22]).is_err());
}

#[tokio::test]
async fn test_tsk202_dcp_unconfigured_device_identify_within_2_5_seconds() {
    let (client_transport, server_transport) = LoopbackRawTransport::create_pair();

    let unconfigured_mac = MacAddress::new([0xAA, 0xBB, 0xCC, 0x11, 0x22, 0x33]);
    let server_config = DcpServerConfig {
        mac: unconfigured_mac,
        station_name: "mythos-unconfigured-rack-42".to_string(),
        vendor_id: VENDOR_ID_FIRMANSYAH,
        device_id: 0x0001,
        device_role: DEVICE_ROLE_IO_DEVICE,
        ip: Ipv4Addr::UNSPECIFIED, // 0.0.0.0
        subnet: Ipv4Addr::new(255, 255, 255, 0),
        gateway: Ipv4Addr::UNSPECIFIED,
    };

    let server = Arc::new(DcpServer::new(server_config));
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let srv_clone = server.clone();
    let srv_handle = tokio::spawn(async move {
        srv_clone
            .run_responder(Arc::new(server_transport), shutdown_rx)
            .await;
    });

    let client = DcpClient::new(MacAddress::new([0x00, 0x50, 0x56, 0xC0, 0x00, 0x01]));

    // SYS-REQ-040 requirement: identifies unconfigured field devices within <= 2.5 seconds
    let start_time = std::time::Instant::now();
    let discovered = client
        .identify_sweep(
            &client_transport,
            None,
            Duration::from_millis(500), // Fast probe timeout
        )
        .await
        .expect("identify sweep should succeed");

    let elapsed = start_time.elapsed();
    assert!(
        elapsed <= Duration::from_millis(2500),
        "Identification took {:?}, exceeding 2.5s requirement",
        elapsed
    );

    assert_eq!(discovered.len(), 1);
    let dev = &discovered[0];
    assert_eq!(dev.mac, unconfigured_mac);
    assert_eq!(dev.station_name, "mythos-unconfigured-rack-42");
    assert_eq!(dev.ip, Ipv4Addr::UNSPECIFIED);
    assert!(!dev.is_configured);
    assert_eq!(dev.vendor_id, VENDOR_ID_FIRMANSYAH);

    // Cleanup
    let _ = shutdown_tx.send(true);
    let _ = srv_handle.await;
}

#[tokio::test]
async fn test_tsk202_dcp_station_name_filter_identify() {
    let (client_transport, server_transport) = LoopbackRawTransport::create_pair();

    let server_config = DcpServerConfig {
        mac: MacAddress::new([0x00, 0x12, 0x34, 0x56, 0x78, 0x90]),
        station_name: "target-plc-alpha".to_string(),
        vendor_id: VENDOR_ID_FIRMANSYAH,
        device_id: 0x0002,
        device_role: DEVICE_ROLE_IO_CONTROLLER,
        ip: Ipv4Addr::new(192, 168, 10, 100),
        subnet: Ipv4Addr::new(255, 255, 255, 0),
        gateway: Ipv4Addr::new(192, 168, 10, 1),
    };

    let server = Arc::new(DcpServer::new(server_config));
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let srv_clone = server.clone();
    let srv_handle = tokio::spawn(async move {
        srv_clone
            .run_responder(Arc::new(server_transport), shutdown_rx)
            .await;
    });

    let client = DcpClient::new(MacAddress::new([0x00, 0x50, 0x56, 0xC0, 0x00, 0x01]));

    // Query for non-matching name should find 0 devices
    let non_matching = client
        .identify_sweep(
            &client_transport,
            Some("different-device"),
            Duration::from_millis(150),
        )
        .await
        .expect("identify sweep");
    assert_eq!(non_matching.len(), 0);

    // Query for exact matching name should find the target device
    let matching = client
        .identify_sweep(
            &client_transport,
            Some("target-plc-alpha"),
            Duration::from_millis(300),
        )
        .await
        .expect("identify sweep");
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].station_name, "target-plc-alpha");
    assert_eq!(matching[0].ip, Ipv4Addr::new(192, 168, 10, 100));

    let _ = shutdown_tx.send(true);
    let _ = srv_handle.await;
}

#[tokio::test]
async fn test_tsk202_dcp_layer2_set_ip_and_signal_led() {
    let (client_transport, server_transport) = LoopbackRawTransport::create_pair();

    let server_mac = MacAddress::new([0x02, 0x42, 0xAC, 0x11, 0x00, 0x02]);
    let server_config = DcpServerConfig {
        mac: server_mac,
        station_name: "et200sp-io-island".to_string(),
        vendor_id: VENDOR_ID_FIRMANSYAH,
        device_id: 0x0001,
        device_role: DEVICE_ROLE_IO_DEVICE,
        ip: Ipv4Addr::UNSPECIFIED,
        subnet: Ipv4Addr::UNSPECIFIED,
        gateway: Ipv4Addr::UNSPECIFIED,
    };

    let server = Arc::new(DcpServer::new(server_config));
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let srv_clone = server.clone();
    let srv_handle = tokio::spawn(async move {
        srv_clone
            .run_responder(Arc::new(server_transport), shutdown_rx)
            .await;
    });

    let client = DcpClient::new(MacAddress::new([0x00, 0x50, 0x56, 0xC0, 0x00, 0x01]));

    // Remote Layer 2 Set-IP command
    let assigned_ip = Ipv4Addr::new(172, 16, 20, 55);
    let assigned_subnet = Ipv4Addr::new(255, 255, 255, 0);
    let assigned_gateway = Ipv4Addr::new(172, 16, 20, 1);

    client
        .set_ip(
            &client_transport,
            server_mac,
            assigned_ip,
            assigned_subnet,
            assigned_gateway,
            true, // permanent
            Duration::from_millis(500),
        )
        .await
        .expect("set-ip over Layer 2 must succeed");

    let cfg_after_set = server.config().await;
    assert_eq!(cfg_after_set.ip, assigned_ip);
    assert_eq!(cfg_after_set.subnet, assigned_subnet);
    assert_eq!(cfg_after_set.gateway, assigned_gateway);

    // Remote Layer 2 Signal-LED command (TIA / PRONETA Flash LED)
    client
        .signal_led(&client_transport, server_mac, Duration::from_millis(500))
        .await
        .expect("signal-led command must succeed");

    assert_eq!(server.blink_count().await, 1);

    let _ = shutdown_tx.send(true);
    let _ = srv_handle.await;
}

#[tokio::test]
async fn test_tsk203_mdns_routable_fallback_discovery() {
    let (client_transport, server_transport) = SimulatedMdnsTransport::create_paired_network();

    let mut txt = HashMap::new();
    txt.insert("sys".to_string(), "mythos".to_string());
    txt.insert("roles".to_string(), "controller,historian".to_string());

    let service = MdnsService {
        instance_name: "MythOS Edge IPC Primary".to_string(),
        service_type: SERVICE_TYPE_MYTHOS.to_string(),
        host: "ipc-01.local".to_string(),
        ip: Some(Ipv4Addr::new(10, 0, 5, 20)),
        port: 4840,
        txt_records: txt,
    };

    let responder = Arc::new(MdnsResponder::new(service));
    let srv_transport = Arc::new(server_transport);

    let resp_clone = responder.clone();
    let srv_clone = srv_transport.clone();
    let srv_task = tokio::spawn(async move {
        while let Ok((data, _)) = srv_clone.recv_multicast().await {
            if let Ok(pkt) = DnsPacket::parse(&data) {
                if let Some(resp) = resp_clone.handle_packet(&pkt) {
                    let _ = srv_clone.send_multicast(&resp.to_bytes()).await;
                }
            }
        }
    });

    let client = MdnsDiscoveryClient::new(Arc::new(client_transport));
    let discovered = client
        .query_services(SERVICE_TYPE_MYTHOS, Duration::from_millis(500))
        .await
        .expect("mdns query must succeed");

    assert_eq!(discovered.len(), 1);
    let found = &discovered[0];
    assert_eq!(found.instance_name, "MythOS Edge IPC Primary");
    assert_eq!(found.service_type, SERVICE_TYPE_MYTHOS);
    assert_eq!(found.port, 4840);
    assert_eq!(found.ip, Some(Ipv4Addr::new(10, 0, 5, 20)));
    assert_eq!(
        found.txt_records.get("sys").map(|s| s.as_str()),
        Some("mythos")
    );

    srv_task.abort();
}

#[tokio::test]
async fn test_discovery_engine_integration_dcp_and_mdns() {
    let (client_raw, server_raw) = LoopbackRawTransport::create_pair();
    let (client_mdns, server_mdns) = SimulatedMdnsTransport::create_paired_network();

    // Setup DCP server
    let dcp_server = Arc::new(DcpServer::new(DcpServerConfig {
        mac: MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]),
        station_name: "edge-box-dcp".to_string(),
        vendor_id: VENDOR_ID_FIRMANSYAH,
        device_id: 0x0001,
        device_role: DEVICE_ROLE_IO_DEVICE,
        ip: Ipv4Addr::new(192, 168, 1, 99),
        subnet: Ipv4Addr::new(255, 255, 255, 0),
        gateway: Ipv4Addr::new(192, 168, 1, 1),
    }));
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let srv_clone = dcp_server.clone();
    tokio::spawn(async move {
        srv_clone
            .run_responder(Arc::new(server_raw), shutdown_rx)
            .await;
    });

    // Setup mDNS responder
    let mdns_responder = Arc::new(MdnsResponder::new(MdnsService {
        instance_name: "edge-box-opcua".to_string(),
        service_type: SERVICE_TYPE_OPCUA.to_string(),
        host: "edge-box.local".to_string(),
        ip: Some(Ipv4Addr::new(192, 168, 1, 99)),
        port: 4840,
        txt_records: HashMap::new(),
    }));
    let mdns_srv_clone = Arc::new(server_mdns);
    let resp_clone = mdns_responder.clone();
    let srv_channel_clone = mdns_srv_clone.clone();
    let mdns_task = tokio::spawn(async move {
        while let Ok((data, _)) = srv_channel_clone.recv_multicast().await {
            if let Ok(pkt) = DnsPacket::parse(&data) {
                if let Some(resp) = resp_clone.handle_packet(&pkt) {
                    let _ = srv_channel_clone.send_multicast(&resp.to_bytes()).await;
                }
            }
        }
    });

    // Create DiscoveryEngine and register DcpDiscoverer and MdnsDiscoverer
    let mut engine = DiscoveryEngine::new();
    let dcp_discoverer = Arc::new(DcpDiscoverer::new(
        MacAddress::new([0x00, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE]),
        Arc::new(client_raw),
    ));
    let mdns_discoverer = Arc::new(MdnsDiscoverer::new(Arc::new(client_mdns)));

    engine.register(dcp_discoverer);
    engine.register(mdns_discoverer);

    let ctx = DiscoveryContext::new("eth0", DiscoveryMode::GuidedDiscovery)
        .with_timeouts(Duration::from_millis(500), Duration::from_millis(2000));

    // Run discovery for profinet-dcp
    let dcp_results = engine
        .run_discovery(&ctx, Some("profinet-dcp"))
        .await
        .expect("dcp discovery");
    assert_eq!(dcp_results.len(), 1);
    assert_eq!(dcp_results[0].protocol, "profinet-dcp");
    assert_eq!(
        dcp_results[0].station_name,
        Some("edge-box-dcp".to_string())
    );

    // Run discovery for mdns
    let mdns_results = engine
        .run_discovery(&ctx, Some("mdns"))
        .await
        .expect("mdns discovery");
    assert_eq!(mdns_results.len(), 1);
    assert_eq!(mdns_results[0].protocol, "mdns");
    assert_eq!(
        mdns_results[0].station_name,
        Some("edge-box-opcua".to_string())
    );

    let _ = shutdown_tx.send(true);
    mdns_task.abort();
}
