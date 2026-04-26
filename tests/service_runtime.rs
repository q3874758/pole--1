use pole_protocol_draft::{ServiceRuntime, ServiceState};

#[test]
fn restarts_stale_service_state_cleanly() {
    let mut runtime = ServiceRuntime::default();
    runtime.mark_starting(1001);
    runtime.mark_stale();

    assert!(runtime.can_recover_without_manual_cleanup());
}

#[test]
fn stale_failed_service_does_not_claim_safe_recovery() {
    let mut runtime = ServiceRuntime::default();
    runtime.mark_failed("port already in use");
    runtime.mark_stale();

    assert!(!runtime.can_recover_without_manual_cleanup());
}

#[test]
fn running_state_tracks_pid_and_clears_stale_flag() {
    let mut runtime = ServiceRuntime::default();
    runtime.mark_starting(7);
    runtime.mark_stale();
    runtime.mark_running(9);

    assert_eq!(runtime.current_pid(), Some(9));
    assert_eq!(runtime.state(), &ServiceState::Running { pid: 9 });
    assert!(!runtime.is_stale());
}

#[test]
fn stopped_state_clears_pid_and_stale_flag() {
    let mut runtime = ServiceRuntime::default();
    runtime.mark_running(42);
    runtime.mark_stale();
    runtime.mark_stopped();

    assert_eq!(runtime.current_pid(), None);
    assert_eq!(runtime.state(), &ServiceState::Stopped);
    assert!(!runtime.is_stale());
}

#[test]
fn observed_running_process_maps_to_running_state() {
    let runtime = ServiceRuntime::from_observed_process(Some(55), true);

    assert!(runtime.is_running());
    assert_eq!(runtime.current_pid(), Some(55));
    assert_eq!(runtime.state(), &ServiceState::Running { pid: 55 });
}

#[test]
fn observed_stale_pid_maps_to_stale_starting_state() {
    let runtime = ServiceRuntime::from_observed_process(Some(55), false);

    assert!(!runtime.is_running());
    assert!(runtime.is_stale());
    assert_eq!(runtime.current_pid(), Some(55));
    assert_eq!(runtime.state(), &ServiceState::Starting { pid: 55 });
    assert!(runtime.can_recover_without_manual_cleanup());
}

#[test]
fn persisted_pid_is_used_when_observed_pid_is_missing() {
    let runtime = ServiceRuntime::from_persisted_observation(None, Some(88), false);

    assert_eq!(runtime.current_pid(), Some(88));
    assert_eq!(runtime.state(), &ServiceState::Starting { pid: 88 });
    assert!(runtime.is_stale());
}

#[test]
fn observed_pid_takes_priority_over_persisted_pid() {
    let runtime = ServiceRuntime::from_persisted_observation(Some(11), Some(88), true);

    assert_eq!(runtime.current_pid(), Some(11));
    assert_eq!(runtime.state(), &ServiceState::Running { pid: 11 });
    assert!(runtime.is_running());
}

#[test]
fn snapshot_reports_running_state_consistently() {
    let runtime = ServiceRuntime::from_observed_process(Some(21), true);
    let snapshot = runtime.snapshot();

    assert_eq!(snapshot.state_label, "running");
    assert_eq!(snapshot.pid, Some(21));
    assert!(!snapshot.stale);
    assert!(!snapshot.recoverable_without_manual_cleanup);
}

#[test]
fn snapshot_reports_stale_starting_state_as_recoverable() {
    let runtime = ServiceRuntime::from_persisted_observation(None, Some(88), false);
    let snapshot = runtime.snapshot();

    assert_eq!(snapshot.state_label, "starting");
    assert_eq!(snapshot.pid, Some(88));
    assert!(snapshot.stale);
    assert!(snapshot.recoverable_without_manual_cleanup);
}
