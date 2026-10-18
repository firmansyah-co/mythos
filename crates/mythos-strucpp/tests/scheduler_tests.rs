use mythos_strucpp::scheduler::{StagedSchedulerConfig, TaskScheduleConfig};
use std::time::Duration;

#[test]
fn test_staging_isolation_sys_req_080() {
    let initial = TaskScheduleConfig {
        name: "TASK0".into(),
        interval: Duration::from_millis(10),
        priority: 80,
        core_affinity: None,
    };

    let staged = StagedSchedulerConfig::new(initial);

    // Initial check: active == draft
    assert_eq!(staged.get_active().interval, Duration::from_millis(10));
    assert_eq!(staged.get_draft().interval, Duration::from_millis(10));

    // Modifying draft from 10ms to 50ms (simulating Monaco canvas edit)
    staged.update_draft(|draft| {
        draft.interval = Duration::from_millis(50);
        draft.priority = 85;
    });

    // Active configuration MUST remain untouched (SYS-REQ-080)
    assert_eq!(staged.get_active().interval, Duration::from_millis(10));
    assert_eq!(staged.get_active().priority, 80);
    assert_eq!(staged.get_draft().interval, Duration::from_millis(50));
    assert_eq!(staged.get_draft().priority, 85);

    // Boundary handshake without request does NOT promote
    assert!(!staged.commit_promotion_at_boundary());
    assert_eq!(staged.get_active().interval, Duration::from_millis(10));

    // Request promotion (simulating user clicking [Deploy / Activate])
    staged.request_promotion();
    assert!(staged.is_promotion_pending());

    // Still active at 10ms before boundary handshake
    assert_eq!(staged.get_active().interval, Duration::from_millis(10));

    // Scan boundary reached: commit promotion
    assert!(staged.commit_promotion_at_boundary());
    assert!(!staged.is_promotion_pending());

    // Now active configuration has transitioned atomically
    assert_eq!(staged.get_active().interval, Duration::from_millis(50));
    assert_eq!(staged.get_active().priority, 85);
}
