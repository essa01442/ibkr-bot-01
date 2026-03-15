use bridge_rx::BridgeRxTask;
use core_types::{Event, EventKind};
use event_bus::EventBus;
use tokio::sync::mpsc;
use tokio::task;

#[tokio::test]
async fn test_python_rust_integration_scenarios() {
    let socket_path = "/tmp/rps_test_uds.sock";

    // Create channels
    let (tx, mut rx) = mpsc::channel(100);
    let (_, dummy_rx) = mpsc::channel(1); // not used

    let bus = EventBus {
        tx,
        rx: dummy_rx,
    };

    let mut bridge_task = BridgeRxTask::new(socket_path, bus).unwrap();

    let (degraded_tx, mut degraded_rx) = mpsc::channel(10);
    bridge_task.set_degraded_notifier(degraded_tx.clone());

    // Run bridge task in background
    let task_handle = task::spawn(async move {
        bridge_task.run().await;
    });

    // Wait for socket to exist
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Run python test script which will connect to `socket_path`
    // pass socket_path as an argument
    let mut python_cmd = std::process::Command::new("python3");

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
    let output = python_cmd.output().expect("Failed to execute python");

    if !output.status.success() {
        println!("Python stderr: {}", String::from_utf8_lossy(&output.stderr));
        println!("Python stdout: {}", String::from_utf8_lossy(&output.stdout));
    }
    assert!(output.status.success(), "Integration tests failed");

    // Validate we received the events
    let event = rx.recv().await.expect("Failed to receive tick event");
    assert!(matches!(event.kind, EventKind::Tick(_)));
    assert_eq!(event.symbol_id.0, 42);

    let hb1 = rx.recv().await.expect("Failed to receive first heartbeat");
    assert!(matches!(hb1.kind, EventKind::Heartbeat));

    let degraded = degraded_rx.recv().await.expect("Failed to receive degraded mode true");
    assert!(degraded, "Should have entered degraded mode");

    let hb2 = rx.recv().await.expect("Failed to receive second heartbeat");
    assert!(matches!(hb2.kind, EventKind::Heartbeat));

    let recovered = degraded_rx.recv().await.expect("Failed to receive degraded mode false");
    assert!(!recovered, "Should have recovered from degraded mode");

    task_handle.abort();
}
