use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn get_replayer_bin() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // Remove exe name
    if path.ends_with("deps") {
        path.pop(); // Remove deps
    }
    path.join("replayer")
}

#[test]
fn test_replayer_runs_without_panic() {
    let bin = get_replayer_bin();
    let sample_csv = "tests/sample_data.csv";
    let out_json = "tests/out1.json";

    // Clean up before run
    let _ = fs::remove_file(out_json);

    let output = Command::new(&bin)
        .arg(sample_csv)
        .arg(out_json)
        .output()
        .expect("Failed to execute replayer");

    assert!(
        output.status.success(),
        "Replayer crashed or failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        PathBuf::from(out_json).exists(),
        "Output JSON was not created"
    );
}

#[test]
fn test_replayer_deterministic() {
    let bin = get_replayer_bin();
    let sample_csv = "tests/sample_data.csv";
    let out1 = "tests/out_det1.json";
    let out2 = "tests/out_det2.json";

    let _ = fs::remove_file(out1);
    let _ = fs::remove_file(out2);

    let _ = Command::new(&bin)
        .arg(sample_csv)
        .arg(out1)
        .output()
        .unwrap();
    let _ = Command::new(&bin)
        .arg(sample_csv)
        .arg(out2)
        .output()
        .unwrap();

    let output = Command::new(&bin)
        .arg("--compare")
        .arg(out1)
        .arg(out2)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "Deterministic comparison failed: {}",
        stdout
    );
    assert!(
        stdout.contains("PASS: ≥ 99% match"),
        "Did not pass 99% threshold"
    );
}

#[test]
fn test_replayer_detects_parameter_change() {
    let bin = get_replayer_bin();
    let sample_csv = "tests/sample_data.csv";
    let out_normal = "tests/out_normal.json";
    let out_altered = "tests/out_altered.json";

    let _ = fs::remove_file(out_normal);
    let _ = fs::remove_file(out_altered);

    // Standard run
    let _ = Command::new(&bin)
        .arg(sample_csv)
        .arg(out_normal)
        .output()
        .unwrap();

    // Create altered config
    let mut config_path = PathBuf::from("configs/default.toml");
    if !config_path.exists() {
        config_path = PathBuf::from("../../configs/default.toml");
    }
    if !config_path.exists() {
        config_path = PathBuf::from("../../../configs/default.toml");
    }
    let config_str = fs::read_to_string(&config_path).unwrap();
    // Change tape threshold to something lower so maybe we pass TapeScoreLow (it scored 10.4).
    // Let's change tape_threshold_normal to 0, which will let it pass TapeScoreLow, but then it'll fail at NetNegative.
    let altered_config =
        config_str.replace("tape_threshold_normal = 72", "tape_threshold_normal = 0");

    let altered_path = "tests/altered.toml";
    fs::write(&altered_path, altered_config).unwrap();

    let _ = Command::new(&bin)
        .arg(sample_csv)
        .arg(out_altered)
        .arg(&altered_path)
        .output()
        .unwrap();

    // Cleanup
    let _ = fs::remove_file(altered_path);

    let output = Command::new(&bin)
        .arg("--compare")
        .arg(out_normal)
        .arg(out_altered)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // They should NOT match, meaning the replayer correctly picked up the parameter change
    assert!(
        stdout.contains("FAIL: match < 99% threshold"),
        "Replayer did not detect parameter change. Stdout: {}",
        stdout
    );
}
