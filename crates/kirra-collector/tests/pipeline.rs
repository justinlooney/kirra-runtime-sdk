// crates/kirra-collector/tests/pipeline.rs
//
// STEP B2 — drive the WHOLE collector pipeline (ingest → stratified sampling →
// join → Parquet → reconciliation) against SYNTHETIC fixtures: hand-built
// capture records for both sources + an in-memory bag. No real rosbag needed
// (C2/C4 deferred until the GPU bench is up).
//
// Asserts: join hit/orphan counts; stratified sampling keeps every intervention
// and samples passes; Parquet partitions by doer_version/source; bulk_ref present
// and heavy frames absent; the orphan-rate gate fails loud.

use std::path::{Path, PathBuf};

use kirra_capture_schema::{
    CaptureOutcome, CaptureRecord, CaptureSource, PoseSnapshot, ProposedCommandSnapshot,
    TrajectoryCaptureExt,
};
use kirra_collector::bag::{BusMessage, InMemoryBag};
use kirra_collector::dataset::read_part_columns_and_rows;
use kirra_collector::{list_parquet_parts, run, CollectorConfig, CollectorError};

// ---- fixtures --------------------------------------------------------------

fn gateway(seq: u64, t_wall_ms: u64, outcome: CaptureOutcome) -> CaptureRecord {
    CaptureRecord {
        decision_seq: seq,
        t_mono_ns: u128::from(seq) * 1000,
        t_wall_ms,
        source: CaptureSource::CommandGateway,
        proposed: Some(ProposedCommandSnapshot {
            linear_velocity_mps: 40.0,
            current_velocity_mps: 40.0,
            steering_angle_deg: 0.0,
            current_steering_angle_deg: 0.0,
            delta_time_s: 0.1,
        }),
        traj: None,
        outcome,
        deny_code: matches!(outcome, CaptureOutcome::Deny).then(|| "NAN_INF_LINEAR_VELOCITY".to_string()),
        safe_value: matches!(outcome, CaptureOutcome::ClampLinear).then_some(35.0),
        mrc: false,
        posture: "NOMINAL".to_string(),
        derate_enabled: false,
    }
}

fn trajectory(seq: u64, t_wall_ms: u64, traj_id: u64, outcome: CaptureOutcome) -> CaptureRecord {
    CaptureRecord {
        decision_seq: seq,
        t_mono_ns: u128::from(seq) * 1000,
        t_wall_ms,
        source: CaptureSource::SlowLoopTrajectory,
        proposed: None,
        traj: Some(TrajectoryCaptureExt {
            asset_id: "ego".to_string(),
            trajectory_id: traj_id,
            objects_ms: 500,
            point_count: 12,
            object_count: 3,
            first_pose: Some(PoseSnapshot { x_m: 0.0, y_m: 0.0, heading_rad: 0.0 }),
            last_pose: Some(PoseSnapshot { x_m: 5.0, y_m: 1.0, heading_rad: 0.1 }),
            target_speed_mps: Some(8.0),
        }),
        outcome,
        deny_code: matches!(outcome, CaptureOutcome::Deny).then(|| "TRAJECTORY_MRC_FALLBACK".to_string()),
        safe_value: None,
        mrc: matches!(outcome, CaptureOutcome::Deny),
        posture: "NOMINAL".to_string(),
        derate_enabled: false,
    }
}

fn bus_msg(t_wall_ms: u64, ver: &str, traj_id: Option<u64>, reff: &str) -> BusMessage {
    BusMessage {
        t_wall_ms,
        doer_version: ver.to_string(),
        asset_id: traj_id.map(|_| "ego".to_string()),
        trajectory_id: traj_id,
        objects_ms: traj_id.map(|_| 500),
        bulk_ref: reff.to_string(),
    }
}

fn unique_out(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("kirra-collector-test-{tag}-{nanos}"))
}

/// The dataset's full column set — pinning this proves NO raw frame/point/object
/// payload column exists (only counts + endpoint poses + bulk_ref).
const EXPECTED_COLUMNS: &[&str] = &[
    "decision_seq", "source", "t_wall_ms", "t_mono_ns", "doer_version", "outcome", "deny_code",
    "safe_value", "mrc", "posture", "derate_enabled", "proposed_linear_velocity_mps",
    "proposed_current_velocity_mps", "proposed_steering_angle_deg",
    "proposed_current_steering_angle_deg", "proposed_delta_time_s", "asset_id", "trajectory_id",
    "objects_ms", "point_count", "object_count", "first_pose_x_m", "first_pose_y_m",
    "first_pose_heading_rad", "last_pose_x_m", "last_pose_y_m", "last_pose_heading_rad",
    "target_speed_mps", "bulk_ref",
];

// ---- tests -----------------------------------------------------------------

#[test]
fn happy_path_joins_both_sources_and_partitions() {
    let out = unique_out("happy");
    let records = vec![
        gateway(0, 1000, CaptureOutcome::Allow),
        gateway(1, 1100, CaptureOutcome::ClampLinear),
        trajectory(0, 2000, 42, CaptureOutcome::Allow),
        trajectory(1, 2100, 43, CaptureOutcome::Deny),
    ];
    let bag = InMemoryBag::new(
        "synthetic",
        vec![
            bus_msg(1005, "model_v1", None, "bag#gw0"),
            bus_msg(1105, "model_v1", None, "bag#gw1"),
            bus_msg(2005, "model_v1", Some(42), "bag#tj0"),
            bus_msg(2105, "model_v1", Some(43), "bag#tj1"),
        ],
    );
    let cfg = CollectorConfig {
        pass_rate: 1.0,
        window_ms: 100,
        max_orphan_rate: 0.0,
        out_dir: out.clone(),
    };

    let recon = run(records, &bag, &cfg).expect("run should succeed");
    assert_eq!(recon.records_in, 4);
    assert_eq!(recon.records_in_command_gateway, 2);
    assert_eq!(recon.records_in_slow_loop_trajectory, 2);
    assert_eq!(recon.interventions_in, 2, "clamp + deny");
    assert_eq!(recon.passes_in, 2, "allow + accept");
    assert_eq!(recon.kept_after_sampling, 4, "pass_rate 1.0 keeps all");
    assert_eq!(recon.joined, 4);
    assert_eq!(recon.orphans, 0);
    assert_eq!(recon.orphan_rate, 0.0);

    // Two partitions: one per source, both under doer_version=model_v1.
    let gw_part = out.join("doer_version=model_v1/source=COMMAND_GATEWAY/part-000.parquet");
    let tj_part = out.join("doer_version=model_v1/source=SLOW_LOOP_TRAJECTORY/part-000.parquet");
    assert!(gw_part.exists(), "gateway partition must exist at {gw_part:?}");
    assert!(tj_part.exists(), "trajectory partition must exist at {tj_part:?}");

    let (cols, rows) = read_part_columns_and_rows(&gw_part).unwrap();
    assert_eq!(rows, 2, "two gateway rows");
    assert_eq!(cols, EXPECTED_COLUMNS, "schema must be exactly the summary columns");
    assert!(cols.contains(&"bulk_ref".to_string()), "bulk_ref present");
    // Heavy frames absent: no raw payload columns, only counts + endpoints.
    for forbidden in ["points", "objects", "object_list", "frame", "frames", "trajectory_points"] {
        assert!(!cols.iter().any(|c| c == forbidden), "no raw `{forbidden}` column");
    }

    let (_cols, tj_rows) = read_part_columns_and_rows(&tj_part).unwrap();
    assert_eq!(tj_rows, 2, "two trajectory rows");

    cleanup(&out);
}

#[test]
fn stratified_sampling_keeps_interventions_drops_passes_at_zero_rate() {
    let out = unique_out("sample0");
    let records = vec![
        gateway(0, 1000, CaptureOutcome::Allow),       // pass → dropped at 0.0
        gateway(1, 1100, CaptureOutcome::ClampLinear), // intervention → kept
        trajectory(0, 2000, 42, CaptureOutcome::Allow), // pass → dropped
        trajectory(1, 2100, 43, CaptureOutcome::Deny),  // intervention → kept
    ];
    let bag = InMemoryBag::new(
        "synthetic",
        vec![
            bus_msg(1105, "model_v1", None, "bag#gw1"),
            bus_msg(2105, "model_v1", Some(43), "bag#tj1"),
        ],
    );
    let cfg = CollectorConfig { pass_rate: 0.0, window_ms: 100, max_orphan_rate: 0.0, out_dir: out.clone() };

    let recon = run(records, &bag, &cfg).expect("run should succeed");
    assert_eq!(recon.passes_in, 2);
    assert_eq!(recon.interventions_in, 2);
    assert_eq!(recon.passes_kept, 0, "pass_rate 0.0 drops every pass");
    assert_eq!(recon.interventions_kept, 2, "every intervention survives");
    assert_eq!(recon.kept_after_sampling, 2);
    assert_eq!(recon.joined, 2);
    assert_eq!(recon.orphans, 0);
    cleanup(&out);
}

#[test]
fn orphan_rate_gate_fails_loud_when_exceeded() {
    let out = unique_out("orphan");
    // One gateway record with NO bus message in the window → orphan.
    let records = vec![gateway(0, 1000, CaptureOutcome::ClampLinear)];
    let bag = InMemoryBag::new("synthetic", vec![bus_msg(99_000, "model_v1", None, "far")]);

    // Ceiling 0.0 → the single orphan (rate 1.0) must fail loud.
    let cfg = CollectorConfig { pass_rate: 1.0, window_ms: 100, max_orphan_rate: 0.0, out_dir: out.clone() };
    match run(records, &bag, &cfg) {
        Err(CollectorError::OrphanRateExceeded { recon, max }) => {
            assert_eq!(recon.orphans, 1);
            assert_eq!(recon.joined, 0);
            assert_eq!(recon.orphan_rate, 1.0);
            assert_eq!(max, 0.0);
        }
        other => panic!("expected OrphanRateExceeded, got {other:?}"),
    }
    cleanup(&out);
}

#[test]
fn orphan_under_ceiling_succeeds() {
    let out = unique_out("orphan_ok");
    // 1 joined + 1 orphan → orphan_rate 0.5, under a 0.75 ceiling.
    let records = vec![
        gateway(0, 1000, CaptureOutcome::ClampLinear),
        gateway(1, 5000, CaptureOutcome::Deny),
    ];
    let bag = InMemoryBag::new("synthetic", vec![bus_msg(1005, "model_v1", None, "bag#gw0")]);
    let cfg = CollectorConfig { pass_rate: 1.0, window_ms: 100, max_orphan_rate: 0.75, out_dir: out.clone() };

    let recon = run(records, &bag, &cfg).expect("under ceiling → ok");
    assert_eq!(recon.joined, 1);
    assert_eq!(recon.orphans, 1);
    assert_eq!(recon.orphan_rate, 0.5);
    cleanup(&out);
}

#[test]
fn distinct_doer_versions_produce_distinct_partitions() {
    let out = unique_out("multiver");
    let records = vec![
        gateway(0, 1000, CaptureOutcome::ClampLinear),
        gateway(1, 2000, CaptureOutcome::ClampLinear),
    ];
    // Same source, two different doer versions on the bus side.
    let bag = InMemoryBag::new(
        "synthetic",
        vec![
            bus_msg(1005, "model_v1", None, "bag#a"),
            bus_msg(2005, "model_v2", None, "bag#b"),
        ],
    );
    let cfg = CollectorConfig { pass_rate: 1.0, window_ms: 100, max_orphan_rate: 0.0, out_dir: out.clone() };

    let recon = run(records, &bag, &cfg).expect("run ok");
    assert_eq!(recon.joined, 2);
    let parts = list_parquet_parts(&out).unwrap();
    assert_eq!(parts.len(), 2, "one partition per doer_version");
    assert!(out.join("doer_version=model_v1/source=COMMAND_GATEWAY/part-000.parquet").exists());
    assert!(out.join("doer_version=model_v2/source=COMMAND_GATEWAY/part-000.parquet").exists());
    cleanup(&out);
}

#[test]
fn read_jsonl_and_dedup_round_trips_through_the_pipeline() {
    let out = unique_out("jsonl");
    let dir = unique_out("jsonl_in");
    std::fs::create_dir_all(&dir).unwrap();
    let capture_path = dir.join("capture.jsonl");

    // Two lines, the SECOND a duplicate (source, decision_seq) of the first.
    let r0 = gateway(0, 1000, CaptureOutcome::ClampLinear);
    let dup = gateway(0, 1000, CaptureOutcome::Allow); // same key → dropped
    let r1 = trajectory(0, 2000, 42, CaptureOutcome::Deny);
    let body = format!(
        "{}\n{}\n\n{}\n", // blank line tolerated
        serde_json::to_string(&r0).unwrap(),
        serde_json::to_string(&dup).unwrap(),
        serde_json::to_string(&r1).unwrap(),
    );
    std::fs::write(&capture_path, body).unwrap();

    let records = kirra_collector::read_jsonl(&[capture_path]).unwrap();
    assert_eq!(records.len(), 3, "read_jsonl reads every non-blank line incl. the dup");

    let bag = InMemoryBag::new(
        "synthetic",
        vec![
            bus_msg(1005, "model_v1", None, "bag#gw0"),
            bus_msg(2005, "model_v1", Some(42), "bag#tj0"),
        ],
    );
    let cfg = CollectorConfig { pass_rate: 1.0, window_ms: 100, max_orphan_rate: 0.0, out_dir: out.clone() };
    let recon = run(records, &bag, &cfg).expect("run ok");
    assert_eq!(recon.duplicates_dropped, 1, "the duplicate (source, decision_seq) is dropped");
    assert_eq!(recon.records_in, 2, "two distinct records survive dedup");
    assert_eq!(recon.joined, 2);
    cleanup(&out);
    cleanup(&dir);
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}
