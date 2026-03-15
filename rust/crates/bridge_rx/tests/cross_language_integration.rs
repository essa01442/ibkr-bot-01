use bridge_rx::BridgeRxTask;
use core_types::{CancelRequest, Event, EventKind, OmsCommand};
use event_bus::EventBus;
use std::os::unix::net::UnixDatagram;
use tokio::sync::mpsc;
use tokio::task;

#[tokio::test]
async fn test_python_rust_integration_scenarios() {
    let socket_path = "/tmp/rps_test_uds.sock";
    let cmd_sock_path = "/tmp/rps_test_commands.sock";

    // Create channels
    let (tx, mut rx) = mpsc::channel(100);
    let (_, dummy_rx) = mpsc::channel(1); // not used

    let bus = EventBus { tx, rx: dummy_rx };

    let mut bridge_task = BridgeRxTask::new(socket_path, bus).unwrap();

    let (degraded_tx, mut degraded_rx) = mpsc::channel(10);
    bridge_task.set_degraded_notifier(degraded_tx.clone());

    // Run bridge task in background
    let task_handle = task::spawn(async move {
        bridge_task.run().await;
    });

    // Wait for socket to exist
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // We will simulate Scenario 2: Cancel Order path here
    let cmd = OmsCommand::CancelOrder(CancelRequest { order_id: 999 });
    let _ = std::fs::remove_file(cmd_sock_path);
    let sender = UnixDatagram::unbound().unwrap();

    // Run python test script which will connect to `socket_path`
    // pass socket_path as an argument
    let mut python_cmd = tokio::process::Command::new("python3");

    let current_dir = std::env::current_dir().unwrap();
    let mut script_path = current_dir.clone();
    if script_path.ends_with("bridge_rx") {
        script_path.pop();
        script_path.pop();
        script_path.pop();
    } else if script_path.ends_with("rust") {
        script_path.pop();
    }
    script_path.push("python");
    script_path.push("integration_tests.py");

    python_cmd.arg(script_path).arg(socket_path);

    let mut child = python_cmd.spawn().unwrap();

    // Let Python start up and bind to command socket
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    let payload = rmp_serde::to_vec_named(&cmd).unwrap();
    let _ = sender.send_to(&payload, cmd_sock_path); // Scenario 2 sent

    let status = child.wait().await.unwrap();

    assert!(status.success(), "Integration tests failed");

    // Validate we received the events
    let event = rx.recv().await.expect("Failed to receive tick event");
    assert!(matches!(event.kind, EventKind::Tick(_)));
    assert_eq!(event.symbol_id.0, 42);

    let fill = rx.recv().await.expect("Failed to receive fill event");
    assert!(
        matches!(fill.kind, EventKind::Fill(_)),
        "Scenario 3 failed: Expected Fill"
    );

    let hb1 = rx.recv().await.expect("Failed to receive first heartbeat");
    assert!(matches!(hb1.kind, EventKind::Heartbeat));

    let degraded = degraded_rx
        .recv()
        .await
        .expect("Failed to receive degraded mode true");
    assert!(degraded, "Should have entered degraded mode");

    let hb2 = rx.recv().await.expect("Failed to receive second heartbeat");
    assert!(matches!(hb2.kind, EventKind::Heartbeat));

    let recovered = degraded_rx
        .recv()
        .await
        .expect("Failed to receive degraded mode false");
    assert!(!recovered, "Should have recovered from degraded mode");

    task_handle.abort();
}
