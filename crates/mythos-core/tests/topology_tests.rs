//! Unit and integration tests for Network Topology Model (TSK-206).

use mythos_core::topology::*;

#[test]
fn test_topology_graph_construction_and_retrieval() {
    let mut topo = NetworkTopology::new();

    let controller = TopologyNode::new("ctrl-1", "PLC_MAIN", NodeType::Controller)
        .with_port("X1-P1", "Port 1", "rj45")
        .with_port("X1-P2", "Port 2", "rj45")
        .with_ip("192.168.1.10", "255.255.255.0", "192.168.1.1");

    let switch = TopologyNode::new("sw-1", "SWITCH_CABINET", NodeType::Switch)
        .with_port("P1", "Port 1", "rj45")
        .with_port("P2", "Port 2", "rj45")
        .with_ip("192.168.1.2", "255.255.255.0", "192.168.1.1");

    topo.add_node(controller).unwrap();
    topo.add_node(switch).unwrap();

    let edge = TopologyEdge::EthernetLink {
        id: "link-1".into(),
        from_node: "ctrl-1".into(),
        from_port: "X1-P1".into(),
        to_node: "sw-1".into(),
        to_port: "P1".into(),
        topology: EthernetBusTopology::Star,
        speed_mbps: Some(100),
    };

    topo.add_edge(edge).unwrap();

    assert_eq!(topo.nodes.len(), 2);
    assert_eq!(topo.edges.len(), 1);
    assert_eq!(topo.get_node("ctrl-1").unwrap().station_name, "PLC_MAIN");
}

#[test]
fn test_topology_validation_detects_ip_collision() {
    let mut topo = NetworkTopology::new();

    let n1 = TopologyNode::new("n1", "DEVICE_A", NodeType::RemoteIo).with_ip(
        "192.168.1.50",
        "255.255.255.0",
        "192.168.1.1",
    );
    let n2 = TopologyNode::new("n2", "DEVICE_B", NodeType::RemoteIo).with_ip(
        "192.168.1.50",
        "255.255.255.0",
        "192.168.1.1",
    );

    topo.add_node(n1).unwrap();
    topo.add_node(n2).unwrap();

    let errors = topo.validate();
    let collision_error = errors.iter().find(|e| e.rule == "SYS-REQ-TOPOLOGY-002");
    assert!(collision_error.is_some(), "Expected IP collision error");
    assert_eq!(collision_error.unwrap().affected_nodes.len(), 2);
}

#[test]
fn test_topology_validation_detects_duplicate_station_name() {
    let mut topo = NetworkTopology::new();

    let n1 = TopologyNode::new("n1", "ET200SP_ISLAND", NodeType::RemoteIo);
    let n2 = TopologyNode::new("n2", "ET200SP_ISLAND", NodeType::RemoteIo);

    topo.add_node(n1).unwrap();
    topo.add_node(n2).unwrap();

    let errors = topo.validate();
    let dup_error = errors.iter().find(|e| e.rule == "SYS-REQ-TOPOLOGY-001");
    assert!(dup_error.is_some(), "Expected duplicate station name error");
}

#[test]
fn test_topology_ip_allocation() {
    let mut topo = NetworkTopology::new();

    topo.add_ip_block(IpBlock {
        subnet: "192.168.1.0/24".into(),
        netmask: "255.255.255.0".into(),
        gateway: "192.168.1.1".into(),
        pool_start: "192.168.1.100".into(),
        pool_end: "192.168.1.105".into(),
    });

    let n1 = TopologyNode::new("n1", "STATION_1", NodeType::RemoteIo).with_ip(
        "192.168.1.100",
        "255.255.255.0",
        "192.168.1.1",
    );
    topo.add_node(n1).unwrap();

    let next_ip = topo.allocate_next_ip("192.168.1.0/24").unwrap();
    assert_eq!(next_ip, "192.168.1.101");
}
