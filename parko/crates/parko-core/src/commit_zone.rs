// parko-core/src/commit_zone.rs
//
// SG5 — map-anchored COMMIT_ZONE_BLOCKED veto (foundation brick under EPIC #106).
//
// SG5 (OCCY_SAFETY_GOALS.md H5, ASIL B): the system shall NOT enter a
// high-consequence commit zone (rail crossing, box junction, narrow bridge)
// without confirmed clearance and a verified exit, and shall not stop within
// one. Safe state: STOP SHORT of the zone, rejecting ≥ ~94 m ahead at the cap.
//
// LOAD-BEARING ROBUSTNESS PROPERTY — "Reject fires from MAP ALONE": a KNOWN zone
// with degraded/absent inputs blocks WITHOUT needing live perception of the
// hazard. The veto is anchored on a map prior, so a perception miss of the
// crossing/junction cannot defeat it.
//
// LAYERING: this brick is the zone model + map-anchored fail-closed gate + entry
// veto over SUPPLIED clearance/exit signals. `clearance_confirmed` and
// `exit_verified` are INPUTS here (synthetic in tests). #107 derives
// exit-clearance from geometry/kinematics (and adds stop-inside-zone
// prevention); #108 derives train / non-yielding-agent conflict. Both replace
// the supplied booleans with computed logic ON TOP of this foundation.
//
// The INPUT health-gating mirrors the gateway SG2 containment `Corridor`
// (confidence / staleness / finiteness → fail-closed degraded). The VERDICT
// lives in parko's own vocabulary (like water / occlusion / impact) — the
// gateway `DenyCode` enum is inside the FROZEN talisman and is never touched.

/// A map prior describing a commit zone on the ego path. The health fields
/// mirror the SG2 containment `Corridor`: an absent / stale / low-confidence /
/// non-finite map is treated as UNHEALTHY (fail-closed), never as "clear".
#[derive(Debug, Clone, Copy)]
pub struct CommitZoneMap {
    /// A mapped commit zone intersects the ego path within the look-ahead
    /// horizon (a MAP prior, not a perception of the hazard).
    pub zone_ahead: bool,
    /// Distance along the ego path to the zone entry (m). Non-finite → veto.
    pub distance_to_zone_m: f64,
    /// Source confidence in `[0.0, 1.0]`. Below `min_confidence` → unhealthy.
    pub confidence: f32,
    /// Age (ms) of the map snapshot vs now. Above `max_age_ms` → unhealthy.
    pub age_ms: u64,
    /// Minimum acceptable confidence for the map to be considered healthy.
    pub min_confidence: f32,
    /// Maximum acceptable staleness (ms) — tied to the per-cycle FTTI.
    pub max_age_ms: u64,
}

impl CommitZoneMap {
    /// True iff the map prior is present, fresh, plausible, and finite —
    /// matching `Corridor::is_healthy`'s conservative semantics. Failure → the
    /// commit-zone gate fails closed (veto).
    pub fn is_healthy(&self) -> bool {
        self.confidence >= self.min_confidence
            && self.age_ms <= self.max_age_ms
            && self.confidence.is_finite()
            && self.distance_to_zone_m.is_finite()
    }
}

/// The commit-zone scene the governor sees this tick. Mirrors the established
/// ABSENT-vs-KNOWN discipline (cf. `WaterScene`, `OcclusionScene`): an absent
/// map source is NOT "no zone".
#[derive(Debug, Clone, Copy)]
pub enum CommitZoneScene {
    /// A healthy map reports no commit zone on the path → no veto.
    NoZone,
    /// A mapped commit zone is ahead, with the (supplied) clearance / exit
    /// confirmations for it. Entry requires BOTH on a HEALTHY map.
    ZoneAhead {
        map: CommitZoneMap,
        /// Clearance into the zone is confirmed (no conflicting traffic / train).
        clearance_confirmed: bool,
        /// A clear exit beyond the zone is verified (won't get stuck inside).
        exit_verified: bool,
    },
    /// The map source is absent / unhealthy this tick → fail-closed VETO.
    /// DISTINCT from `NoZone`: an absent map is not "no zone" (the #238 trap and
    /// the literal "Reject fires from map alone" requirement).
    Unknown,
}

/// Config for the commit-zone gate. `look_ahead_m` is a PARAMETER with a
/// conservative VALIDATION-PENDING default tied to the SG5 ≈ 94 m basis
/// (SSD = v·t_react + v²/2a at the 22.35 m/s cap) — NOT a certified constant
/// (same honesty as #98's water thresholds). It derates with the cap under
/// degraded conditions (handled upstream).
#[derive(Debug, Clone, Copy)]
pub struct CommitZoneCfg {
    /// Actionable look-ahead (m): a zone farther than this is not yet a decision.
    pub look_ahead_m: f64,
}

impl Default for CommitZoneCfg {
    fn default() -> Self {
        // VALIDATION-PENDING placeholder — the SG5 / SG4 ≈ 94 m look-ahead basis.
        Self { look_ahead_m: 94.0 }
    }
}

/// SG5 — must the governor BLOCK entry to this commit zone (stop short)?
///
/// `true`  = veto (COMMIT_ZONE_BLOCKED; the governor stops short of the zone);
/// `false` = no veto (the planner proceeds — no zone, or entry is permitted).
///
/// Lattice:
///   * `NoZone`   → `false` (healthy map, no zone).
///   * `Unknown`  → `true`  (fail-closed; absent map ≠ no zone — Reject from map alone).
///   * `ZoneAhead`→ a non-finite distance vetoes; a zone BEYOND the look-ahead
///     horizon is not yet actionable (no veto); a zone WITHIN the horizon is
///     blocked UNLESS the map is HEALTHY **and** `clearance_confirmed` **and**
///     `exit_verified`. Either confirmation missing, or a degraded map, → block.
///     (Health gates the confirmations — a degraded map cannot earn entry.)
// SAFETY: SG5 | REQ: commit-zone-map-anchored-block | TEST: test_map_prior_perception_miss_unknown_vetoes,test_gate_down_clearance_unconfirmed_vetoes,test_no_verified_exit_vetoes,test_both_confirmed_healthy_no_veto,test_unhealthy_map_with_confirmations_still_vetoes,test_no_zone_distinct_from_unknown,test_nonfinite_distance_vetoes,test_horizon_boundary,test_beyond_horizon_no_veto
pub fn commit_zone_blocked(scene: &CommitZoneScene, cfg: &CommitZoneCfg) -> bool {
    match *scene {
        CommitZoneScene::NoZone => false,
        CommitZoneScene::Unknown => true,
        CommitZoneScene::ZoneAhead {
            map,
            clearance_confirmed,
            exit_verified,
        } => {
            // A non-finite distance can never be trusted as "beyond horizon".
            if !map.distance_to_zone_m.is_finite() {
                return true;
            }
            // Beyond the actionable horizon → not yet a decision (no veto).
            if map.distance_to_zone_m > cfg.look_ahead_m {
                return false;
            }
            // Within horizon: entry requires a HEALTHY map AND both confirmations.
            !(map.is_healthy() && clearance_confirmed && exit_verified)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_map(distance_m: f64) -> CommitZoneMap {
        CommitZoneMap {
            zone_ahead: true,
            distance_to_zone_m: distance_m,
            confidence: 0.95,
            age_ms: 50,
            min_confidence: 0.5,
            max_age_ms: 1_000,
        }
    }

    fn cfg() -> CommitZoneCfg {
        CommitZoneCfg::default() // look_ahead = 94.0 m
    }

    /// Confirmed entry on a healthy map within the horizon → permitted.
    fn confirmed_zone(distance_m: f64) -> CommitZoneScene {
        CommitZoneScene::ZoneAhead {
            map: healthy_map(distance_m),
            clearance_confirmed: true,
            exit_verified: true,
        }
    }

    /// "Reject fires from MAP ALONE": an absent / unhealthy map source vetoes —
    /// no live perception of the crossing needed.
    #[test]
    fn test_map_prior_perception_miss_unknown_vetoes() {
        assert!(commit_zone_blocked(&CommitZoneScene::Unknown, &cfg()),
            "an absent/unhealthy map must veto (Reject from map alone)");
    }

    /// Gate-down: clearance not confirmed → veto, even with a verified exit.
    #[test]
    fn test_gate_down_clearance_unconfirmed_vetoes() {
        let s = CommitZoneScene::ZoneAhead {
            map: healthy_map(50.0), clearance_confirmed: false, exit_verified: true,
        };
        assert!(commit_zone_blocked(&s, &cfg()), "unconfirmed clearance must veto");
    }

    /// No verified exit → veto (the no-stuck-inside guard at entry).
    #[test]
    fn test_no_verified_exit_vetoes() {
        let s = CommitZoneScene::ZoneAhead {
            map: healthy_map(50.0), clearance_confirmed: true, exit_verified: false,
        };
        assert!(commit_zone_blocked(&s, &cfg()), "no verified exit must veto");
    }

    /// Both confirmed on a healthy map within horizon → NO veto (no over-block).
    #[test]
    fn test_both_confirmed_healthy_no_veto() {
        assert!(!commit_zone_blocked(&confirmed_zone(50.0), &cfg()),
            "a healthy, clearance-confirmed, exit-verified zone permits entry");
    }

    /// Health gates the confirmations: a degraded map with BOTH confirmations
    /// STILL vetoes (a degraded map cannot earn entry).
    #[test]
    fn test_unhealthy_map_with_confirmations_still_vetoes() {
        // low confidence
        let low_conf = CommitZoneScene::ZoneAhead {
            map: CommitZoneMap { confidence: 0.1, ..healthy_map(50.0) },
            clearance_confirmed: true, exit_verified: true,
        };
        assert!(commit_zone_blocked(&low_conf, &cfg()), "low-confidence map must veto despite confirmations");
        // stale
        let stale = CommitZoneScene::ZoneAhead {
            map: CommitZoneMap { age_ms: 999_999, ..healthy_map(50.0) },
            clearance_confirmed: true, exit_verified: true,
        };
        assert!(commit_zone_blocked(&stale, &cfg()), "stale map must veto despite confirmations");
    }

    /// NoZone and Unknown are DISTINCT outcomes.
    #[test]
    fn test_no_zone_distinct_from_unknown() {
        assert!(!commit_zone_blocked(&CommitZoneScene::NoZone, &cfg()));
        assert!(commit_zone_blocked(&CommitZoneScene::Unknown, &cfg()));
        assert_ne!(
            commit_zone_blocked(&CommitZoneScene::NoZone, &cfg()),
            commit_zone_blocked(&CommitZoneScene::Unknown, &cfg()),
            "NoZone (healthy, clear) and Unknown (absent map) must differ"
        );
    }

    /// A non-finite distance vetoes (NaN discipline, as in #98/#102).
    #[test]
    fn test_nonfinite_distance_vetoes() {
        for bad in [f64::NAN, f64::INFINITY] {
            let s = confirmed_zone(bad);
            assert!(commit_zone_blocked(&s, &cfg()), "non-finite distance must veto ({bad})");
        }
    }

    /// Hand-checked horizon boundary (look_ahead = 94.0): a confirmed zone
    /// EXACTLY at the horizon is within (decision made → permitted because
    /// confirmed); the SAME distance unconfirmed vetoes; just beyond is not yet
    /// actionable.
    #[test]
    fn test_horizon_boundary() {
        // exactly at horizon, confirmed → within horizon, permitted (no veto).
        assert!(!commit_zone_blocked(&confirmed_zone(94.0), &cfg()),
            "a confirmed zone exactly at the horizon is actionable and permitted");
        // exactly at horizon, unconfirmed → within horizon → veto.
        let at_unconfirmed = CommitZoneScene::ZoneAhead {
            map: healthy_map(94.0), clearance_confirmed: false, exit_verified: true,
        };
        assert!(commit_zone_blocked(&at_unconfirmed, &cfg()),
            "an unconfirmed zone exactly at the horizon must veto (within horizon)");
    }

    /// A zone just beyond the horizon is not yet a decision (no veto), even
    /// unconfirmed.
    #[test]
    fn test_beyond_horizon_no_veto() {
        let beyond = CommitZoneScene::ZoneAhead {
            map: healthy_map(94.0 + 1e-6), clearance_confirmed: false, exit_verified: false,
        };
        assert!(!commit_zone_blocked(&beyond, &cfg()),
            "a zone beyond the look-ahead horizon is not yet actionable");
    }
}
