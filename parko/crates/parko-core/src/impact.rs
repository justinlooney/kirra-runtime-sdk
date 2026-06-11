// parko-core/src/impact.rs
//
// SG6 — post-collision impact latch (IMU + contact + vanished-object fusion, #102).
//
// SG6 (OCCY_SAFETY_GOALS.md H6 — ASIL A, developed to elevated rigor per owner
// decision): after a *detected collision with unconfirmed clearance* — a person
// may be under / near the vehicle — the governor IMMOBILIZES in place and
// executes NO further motion until clearance is confirmed, ≤ 1 cycle from impact.
//
// The mechanism is a post-collision LATCH that is **sticky-toward-safe** (mirrors
// `control_loop`'s `EmergencyStop`: once in the safe state, clean evidence does
// not pull it back out) and is cleared ONLY by an explicit clearance signal.

/// One tick of impact evidence. Synthetic in tests; real sensor / agent-diff
/// ingestion is DEFERRED (see the PR notes).
#[derive(Debug, Clone, Copy)]
pub struct ImpactEvidence {
    /// IMU deceleration-spike magnitude (m/s²). A NON-FINITE value does NOT latch
    /// on its own (an IMU glitch must not immobilize the vehicle) — but it is
    /// NEVER read as a confident "no impact" and NEVER suppresses a contact /
    /// vanished latch (see [`is_impact`]).
    pub imu_accel_spike_mps2: f64,
    /// A physical contact sensor fired — a definitive impact.
    pub contact_sensor: bool,
    /// A close-range tracked agent vanished between frames (the
    /// person-under-vehicle case). Supplied as a flag here; the stateful
    /// `AgentScene`-diff that derives it is DEFERRED. Latches ALONE, per SG6.
    pub vanished_object: bool,
}

/// Fusion config. `spike_threshold_mps2` is a PARAMETER with a conservative
/// **VALIDATION-PENDING** default — a track-test / SOTIF-derived value, NOT a
/// certified constant (the same honesty as #98's water thresholds).
#[derive(Debug, Clone, Copy)]
pub struct ImpactCfg {
    /// IMU deceleration magnitude (m/s²) above which a *finite* spike is read as
    /// a collision-grade impact.
    pub spike_threshold_mps2: f64,
}

impl Default for ImpactCfg {
    fn default() -> Self {
        // VALIDATION-PENDING placeholder — a hard, collision-grade deceleration.
        // NOT a certified value.
        Self { spike_threshold_mps2: 30.0 }
    }
}

/// Conservative, fail-closed fusion: an impact is declared iff **ANY** signal
/// fires —
///   * `contact_sensor` (definitive), OR
///   * a *finite* IMU spike above the threshold (hard decel), OR
///   * `vanished_object` (person-under-vehicle — latches alone, per SG6).
///
/// NaN discipline (the subtle one — do NOT fail open): the IMU term is
/// `is_finite() && > threshold`. The `is_finite()` gate makes the non-latch on a
/// glitch **explicit**, rather than relying on `NaN > threshold` being `false`
/// (the implicit fail-open trap). Because fusion is an OR, a non-finite IMU value
/// never suppresses a `contact_sensor` / `vanished_object` latch and is never
/// treated as a confident "no impact".
pub fn is_impact(evidence: &ImpactEvidence, cfg: &ImpactCfg) -> bool {
    evidence.contact_sensor
        || (evidence.imu_accel_spike_mps2.is_finite()
            && evidence.imu_accel_spike_mps2 > cfg.spike_threshold_mps2)
        || evidence.vanished_object
}

/// Sticky-toward-safe post-collision latch (SG6). Once an impact is observed it
/// STAYS latched — subsequent clean evidence never clears it; only an explicit
/// clearance signal does.
// SAFETY: SG6 | REQ: post-collision-impact-latch | TEST: test_contact_latches,test_finite_spike_over_threshold_latches,test_vanished_object_latches_alone,test_no_signals_no_latch,test_latch_is_sticky,test_explicit_clearance_clears,test_nonfinite_imu_no_spurious_latch,test_nonfinite_does_not_suppress_contact_or_vanished,test_nonfinite_does_not_clear_a_latch,test_spike_threshold_boundary
#[derive(Debug, Clone, Default)]
pub struct ImpactLatch {
    latched: bool,
}

impl ImpactLatch {
    pub fn new() -> Self {
        Self { latched: false }
    }

    /// True while latched — the governor must immobilize.
    pub fn is_latched(&self) -> bool {
        self.latched
    }

    /// Observe one tick of evidence. If it fuses to an impact, latch. STICKY:
    /// once latched this never un-latches on clean evidence — only
    /// [`clear`](Self::clear) with an explicit clearance signal does.
    pub fn observe(&mut self, evidence: &ImpactEvidence, cfg: &ImpactCfg) {
        if is_impact(evidence, cfg) {
            self.latched = true;
        }
        // else: NO-OP — never clears on clean (or non-finite) evidence.
    }

    /// Clear the latch ONLY on an explicit clearance signal (`true`). A `false`
    /// is a no-op (it never re-asserts motion).
    ///
    /// LOW-LEVEL PRIMITIVE — do not call this for production clearance. This is
    /// the inner mechanism; `true` here trusts the caller unconditionally, which
    /// is exactly the gap SS-003 forbids. Production clearance MUST go through
    /// [`ClearanceLoop::try_clear`] (#103), which admits ONLY a well-formed
    /// [`OperatorClearanceGrant`]. `ClearanceLoop` (and #263's
    /// `RecordedImpactLatch`) own an `ImpactLatch` and call this internally; the
    /// method stays public so those wrappers — and the existing #102/#263 APIs —
    /// keep working.
    pub fn clear(&mut self, clearance: bool) {
        if clearance {
            self.latched = false;
        }
    }
}

/// Default ceiling on how old a clearance grant may be (ms) before it is stale.
/// VALIDATION-PENDING conservative placeholder — a grant is a fresh, deliberate
/// operator act, so the window is short; tune on integration.
pub const DEFAULT_MAX_GRANT_AGE_MS: u64 = 60_000;

/// An operator's clearance authorization for a post-collision latch (SG6 / #103).
///
/// LAYERING (the named boundary): parko CANNOT authenticate an operator —
/// authentication lives in the verifier / `kirra_core` reset mechanism (#255,
/// `KIRRA_SUPERVISOR_RESET_KEY`). This type enforces the STRUCTURE only: clearance
/// is admissible ONLY via a well-formed grant, no other path. The integrator /
/// verifier is responsible for issuing a grant ONLY after it has authenticated
/// the operator — that obligation is an assumption of use, not enforced here.
#[derive(Debug, Clone)]
pub struct OperatorClearanceGrant {
    /// The clearing operator's identifier (audit subject). Must be non-empty.
    pub operator_id: String,
    /// Wall-clock time (ms) the grant was issued. Checked against `now_ms`:
    /// must be `<= now` (no future-dating) and within `max_grant_age_ms`.
    pub granted_at_ms: u64,
}

impl OperatorClearanceGrant {
    /// Structural validity (NOT authentication). `true` iff: `operator_id` is
    /// non-empty; `granted_at_ms <= now_ms` (a FUTURE-dated grant is malformed);
    /// and the grant is not older than `max_grant_age_ms` (age boundary is
    /// INCLUSIVE — a grant exactly `max_grant_age_ms` old is still well-formed).
    /// `now_ms` is supplied (no `SystemTime::now()` here — testability).
    pub fn is_well_formed(&self, now_ms: u64, max_grant_age_ms: u64) -> bool {
        if self.operator_id.is_empty() {
            return false;
        }
        if self.granted_at_ms > now_ms {
            return false; // future-dated → malformed
        }
        // u64 subtraction is safe: granted_at_ms <= now_ms here.
        let age_ms = now_ms - self.granted_at_ms;
        age_ms <= max_grant_age_ms // inclusive boundary
    }
}

/// Why a [`ClearanceLoop::try_clear`] attempt was rejected. The state is left
/// UNCHANGED on every rejection (still immobilized).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClearanceRejection {
    /// The grant was not well-formed (empty id / future-dated / stale).
    MalformedGrant,
    /// There was nothing to clear — the loop is in `Normal` (not immobilized).
    /// A clear attempt on `Normal` is a no-op recorded as a rejection, never a
    /// silent success (it would otherwise mask a logic error upstream).
    NotImmobilized,
}

impl ClearanceRejection {
    /// A short, stable reason code for audit bodies.
    pub fn reason_code(&self) -> &'static str {
        match self {
            ClearanceRejection::MalformedGrant => "malformed_grant",
            ClearanceRejection::NotImmobilized => "not_immobilized",
        }
    }
}

/// The lifecycle state of the SG6 clearance loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearanceState {
    /// No active impact — motion permitted (by this check).
    Normal,
    /// Impact fused this tick but the once-per-incident escalation edge has not
    /// yet been raised. Transient: the next `observe` raises escalation.
    /// Immobilized.
    Latched,
    /// The operator-escalation signal has been raised for this incident.
    /// Immobilized; awaiting a well-formed grant.
    EscalationRaised,
}

/// SG6 — the clearance-confirmation + operator-escalation state machine (#103).
///
/// Wraps an [`ImpactLatch`] and enforces the SS-003 "human intervention required"
/// STRUCTURE that the bare latch cannot: once immobilized, the ONLY transition
/// back to `Normal` is [`try_clear`](Self::try_clear) with a well-formed
/// [`OperatorClearanceGrant`]. Clean evidence never clears (the inner latch
/// guarantees this; the wrapper preserves it).
///
/// Lifecycle: `Normal` --impact--> `Latched` --next observe--> `EscalationRaised`
/// --well-formed grant--> `Normal`. The `Latched → EscalationRaised` edge is the
/// distinct, once-per-incident RAISED signal ([`escalation_pending`]).
///
/// THE INVARIANT: there is NO method, evidence pattern, or path that leaves
/// `Latched` / `EscalationRaised` for `Normal` except `try_clear` with a
/// well-formed grant.
// SAFETY: SG6 | REQ: clearance-confirmation-loop | TEST: test_clean_evidence_never_clears_loop,test_malformed_grants_rejected_still_immobilized,test_well_formed_grant_clears,test_escalation_raised_once_per_incident,test_reimpact_during_escalation_no_second_raise,test_cleared_then_new_impact_raises_again,test_grant_on_normal_rejected,test_veto_active_in_both_latched_states,test_grant_age_boundary_inclusive,test_future_dated_grant_malformed
#[derive(Debug, Clone)]
pub struct ClearanceLoop {
    latch: ImpactLatch,
    state: ClearanceState,
    /// Whether the once-per-incident escalation edge has been emitted, so a
    /// re-impact while `EscalationRaised` does not double-raise.
    escalation_emitted: bool,
}

impl Default for ClearanceLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl ClearanceLoop {
    pub fn new() -> Self {
        Self {
            latch: ImpactLatch::new(),
            state: ClearanceState::Normal,
            escalation_emitted: false,
        }
    }

    /// The current lifecycle state.
    pub fn state(&self) -> ClearanceState {
        self.state
    }

    /// True while the vehicle must be immobilized — `Latched` OR
    /// `EscalationRaised`. Feeds the existing motion veto unchanged.
    pub fn is_immobilized(&self) -> bool {
        matches!(
            self.state,
            ClearanceState::Latched | ClearanceState::EscalationRaised
        )
    }

    /// True once the operator-escalation has been raised for the active incident
    /// (the operator-UI signal). False in `Normal` and `Latched`.
    pub fn escalation_pending(&self) -> bool {
        matches!(self.state, ClearanceState::EscalationRaised)
    }

    /// Observe one tick. Delegates fusion to the inner [`ImpactLatch`]:
    /// * `Normal` + fused impact → `Latched` (a new incident).
    /// * `Latched` → `EscalationRaised` (raise the once-per-incident edge),
    ///   whether or not this tick also fused (the latch stays latched regardless).
    /// * `EscalationRaised` → stays (no double-raise on re-impact).
    ///
    /// `now_ms` is accepted for signature symmetry / future use; fusion itself is
    /// time-independent.
    pub fn observe(&mut self, evidence: &ImpactEvidence, cfg: &ImpactCfg, _now_ms: u64) {
        self.latch.observe(evidence, cfg);
        match self.state {
            ClearanceState::Normal => {
                if self.latch.is_latched() {
                    self.state = ClearanceState::Latched;
                    self.escalation_emitted = false;
                }
            }
            ClearanceState::Latched => {
                // The transient Latched state escalates on the next observation.
                self.state = ClearanceState::EscalationRaised;
                self.escalation_emitted = true;
            }
            ClearanceState::EscalationRaised => {
                // Re-impact stays escalated — no second raise.
            }
        }
    }

    /// The ONLY path back to `Normal`. Admits clearance iff the loop is currently
    /// immobilized AND `grant` is well-formed; otherwise returns a
    /// [`ClearanceRejection`] and leaves the state UNCHANGED.
    ///
    /// On success it clears the inner latch via the low-level primitive and
    /// returns to `Normal` (the incident is over; a future impact starts fresh).
    pub fn try_clear(
        &mut self,
        grant: &OperatorClearanceGrant,
        now_ms: u64,
        max_grant_age_ms: u64,
    ) -> Result<(), ClearanceRejection> {
        if !self.is_immobilized() {
            return Err(ClearanceRejection::NotImmobilized);
        }
        if !grant.is_well_formed(now_ms, max_grant_age_ms) {
            return Err(ClearanceRejection::MalformedGrant);
        }
        self.latch.clear(true);
        self.state = ClearanceState::Normal;
        self.escalation_emitted = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ImpactCfg {
        ImpactCfg::default() // spike_threshold = 30.0
    }

    fn clean() -> ImpactEvidence {
        ImpactEvidence { imu_accel_spike_mps2: 0.5, contact_sensor: false, vanished_object: false }
    }

    #[test]
    fn test_contact_latches() {
        let e = ImpactEvidence { contact_sensor: true, ..clean() };
        assert!(is_impact(&e, &cfg()));
        let mut l = ImpactLatch::new();
        l.observe(&e, &cfg());
        assert!(l.is_latched());
    }

    #[test]
    fn test_finite_spike_over_threshold_latches() {
        let e = ImpactEvidence { imu_accel_spike_mps2: 45.0, ..clean() };
        let mut l = ImpactLatch::new();
        l.observe(&e, &cfg());
        assert!(l.is_latched(), "a finite spike above the threshold latches");
    }

    /// SG6: the vanished-object (person-under-vehicle) case latches ALONE.
    #[test]
    fn test_vanished_object_latches_alone() {
        let e = ImpactEvidence { vanished_object: true, ..clean() };
        let mut l = ImpactLatch::new();
        l.observe(&e, &cfg());
        assert!(l.is_latched(), "a vanished close-range agent latches on its own");
    }

    #[test]
    fn test_no_signals_no_latch() {
        let mut l = ImpactLatch::new();
        l.observe(&clean(), &cfg());
        assert!(!l.is_latched(), "no signals → no latch");
    }

    /// THE KEY ASSERTION: once latched, clean evidence does NOT clear it.
    #[test]
    fn test_latch_is_sticky() {
        let mut l = ImpactLatch::new();
        l.observe(&ImpactEvidence { contact_sensor: true, ..clean() }, &cfg());
        assert!(l.is_latched());
        // Subsequent clean ticks must NOT un-latch.
        l.observe(&clean(), &cfg());
        l.observe(&clean(), &cfg());
        assert!(l.is_latched(), "latch is sticky — clean evidence must not clear it");
    }

    #[test]
    fn test_explicit_clearance_clears() {
        let mut l = ImpactLatch::new();
        l.observe(&ImpactEvidence { contact_sensor: true, ..clean() }, &cfg());
        assert!(l.is_latched());
        l.clear(false); // a non-clearance is a no-op
        assert!(l.is_latched(), "clear(false) must not release the latch");
        l.clear(true); // explicit clearance
        assert!(!l.is_latched(), "an explicit clearance signal clears the latch");
    }

    /// A non-finite IMU spike alone does NOT latch (no immobilizing on a glitch).
    #[test]
    fn test_nonfinite_imu_no_spurious_latch() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let e = ImpactEvidence { imu_accel_spike_mps2: bad, ..clean() };
            assert!(!is_impact(&e, &cfg()), "non-finite IMU alone must not latch ({bad})");
        }
    }

    /// A non-finite IMU spike must NEVER suppress a contact / vanished latch
    /// (fusion is OR; the bad reading just contributes `false` to the IMU term).
    #[test]
    fn test_nonfinite_does_not_suppress_contact_or_vanished() {
        let with_contact = ImpactEvidence { imu_accel_spike_mps2: f64::NAN, contact_sensor: true, vanished_object: false };
        assert!(is_impact(&with_contact, &cfg()), "NaN IMU must not suppress a contact latch");
        let with_vanished = ImpactEvidence { imu_accel_spike_mps2: f64::NAN, contact_sensor: false, vanished_object: true };
        assert!(is_impact(&with_vanished, &cfg()), "NaN IMU must not suppress a vanished latch");
    }

    /// A non-finite reading must NOT read as a clean "no impact" that releases an
    /// existing latch — observing it while latched keeps it latched.
    #[test]
    fn test_nonfinite_does_not_clear_a_latch() {
        let mut l = ImpactLatch::new();
        l.observe(&ImpactEvidence { contact_sensor: true, ..clean() }, &cfg());
        assert!(l.is_latched());
        l.observe(&ImpactEvidence { imu_accel_spike_mps2: f64::NAN, contact_sensor: false, vanished_object: false }, &cfg());
        assert!(l.is_latched(), "a non-finite reading must not release the latch (not a clean 'no impact')");
    }

    /// Hand-checked boundary: a spike EXACTLY at the threshold does NOT latch
    /// (strict `>`); one ulp above does.
    #[test]
    fn test_spike_threshold_boundary() {
        let at = ImpactEvidence { imu_accel_spike_mps2: 30.0, ..clean() };
        assert!(!is_impact(&at, &cfg()), "spike exactly at threshold must NOT latch (strict >)");
        let above = ImpactEvidence { imu_accel_spike_mps2: 30.0 + 1e-6, ..clean() };
        assert!(is_impact(&above, &cfg()), "spike just above threshold latches");
    }

    // ───────────────────── #103 clearance-confirmation loop ─────────────────

    const MAX_AGE: u64 = DEFAULT_MAX_GRANT_AGE_MS; // 60_000

    fn contact() -> ImpactEvidence {
        ImpactEvidence { contact_sensor: true, ..clean() }
    }
    /// A well-formed grant issued `age_ms` before `now`.
    fn grant(now: u64, age_ms: u64) -> OperatorClearanceGrant {
        OperatorClearanceGrant { operator_id: "op-7".into(), granted_at_ms: now - age_ms }
    }
    /// Drive a fresh loop into EscalationRaised (impact, then one more tick).
    fn escalated() -> ClearanceLoop {
        let mut l = ClearanceLoop::new();
        l.observe(&contact(), &cfg(), 1_000); // Normal → Latched
        l.observe(&clean(), &cfg(), 1_001); // Latched → EscalationRaised
        assert_eq!(l.state(), ClearanceState::EscalationRaised);
        l
    }

    /// THE INVARIANT (part 1): clean evidence over many ticks never clears the
    /// loop — it stays immobilized.
    #[test]
    fn test_clean_evidence_never_clears_loop() {
        let mut l = escalated();
        for t in 0..50 {
            l.observe(&clean(), &cfg(), 2_000 + t);
            assert!(l.is_immobilized(), "clean evidence must never release the loop");
        }
    }

    /// THE INVARIANT (part 2): every malformed grant is rejected and leaves the
    /// state unchanged (still immobilized).
    #[test]
    fn test_malformed_grants_rejected_still_immobilized() {
        let now = 100_000u64;
        let bad = [
            OperatorClearanceGrant { operator_id: String::new(), granted_at_ms: now }, // empty id
            OperatorClearanceGrant { operator_id: "op".into(), granted_at_ms: now + 5 }, // future
            OperatorClearanceGrant { operator_id: "op".into(), granted_at_ms: now - (MAX_AGE + 1) }, // stale
        ];
        for g in bad {
            let mut l = escalated();
            let r = l.try_clear(&g, now, MAX_AGE);
            assert_eq!(r, Err(ClearanceRejection::MalformedGrant), "malformed grant must be rejected: {g:?}");
            assert!(l.is_immobilized(), "state must be unchanged after a rejected grant");
            assert_eq!(l.state(), ClearanceState::EscalationRaised);
        }
    }

    /// ONLY a well-formed grant transitions back to Normal.
    #[test]
    fn test_well_formed_grant_clears() {
        let now = 100_000u64;
        let mut l = escalated();
        assert!(l.try_clear(&grant(now, 10), now, MAX_AGE).is_ok());
        assert_eq!(l.state(), ClearanceState::Normal);
        assert!(!l.is_immobilized(), "a well-formed grant releases the loop");
        assert!(!l.escalation_pending());
    }

    /// Escalation is a once-per-incident rising edge: it raises exactly once,
    /// then stays pending across many immobilized ticks.
    #[test]
    fn test_escalation_raised_once_per_incident() {
        let mut l = ClearanceLoop::new();
        assert!(!l.escalation_pending());
        l.observe(&contact(), &cfg(), 1_000);
        assert_eq!(l.state(), ClearanceState::Latched);
        assert!(!l.escalation_pending(), "not yet raised in the transient Latched tick");
        l.observe(&clean(), &cfg(), 1_001);
        assert!(l.escalation_pending(), "raised on the next observation");
        // stays pending, no oscillation
        for t in 0..10 {
            l.observe(&clean(), &cfg(), 1_100 + t);
            assert!(l.escalation_pending());
        }
    }

    /// Re-impact while EscalationRaised does not raise a second time.
    #[test]
    fn test_reimpact_during_escalation_no_second_raise() {
        let mut l = escalated();
        l.observe(&contact(), &cfg(), 3_000); // re-impact
        assert_eq!(l.state(), ClearanceState::EscalationRaised, "re-impact stays escalated");
        assert!(l.escalation_pending());
    }

    /// Cleared, then a new impact, raises a NEW escalation (a distinct incident).
    #[test]
    fn test_cleared_then_new_impact_raises_again() {
        let now = 100_000u64;
        let mut l = escalated();
        l.try_clear(&grant(now, 10), now, MAX_AGE).unwrap();
        assert_eq!(l.state(), ClearanceState::Normal);
        // New incident.
        l.observe(&contact(), &cfg(), now + 1_000);
        assert_eq!(l.state(), ClearanceState::Latched);
        l.observe(&clean(), &cfg(), now + 1_001);
        assert!(l.escalation_pending(), "a new impact after clearance raises a fresh escalation");
    }

    /// A clear attempt on Normal is rejected (NotImmobilized), not silently
    /// absorbed.
    #[test]
    fn test_grant_on_normal_rejected() {
        let now = 100_000u64;
        let mut l = ClearanceLoop::new();
        assert_eq!(l.state(), ClearanceState::Normal);
        let r = l.try_clear(&grant(now, 10), now, MAX_AGE);
        assert_eq!(r, Err(ClearanceRejection::NotImmobilized), "clearing Normal must be a recorded rejection");
        assert_eq!(l.state(), ClearanceState::Normal);
    }

    /// The veto (is_immobilized) is active in BOTH Latched and EscalationRaised,
    /// and released only after a grant.
    #[test]
    fn test_veto_active_in_both_latched_states() {
        let mut l = ClearanceLoop::new();
        assert!(!l.is_immobilized()); // Normal
        l.observe(&contact(), &cfg(), 1_000);
        assert_eq!(l.state(), ClearanceState::Latched);
        assert!(l.is_immobilized(), "veto active in Latched");
        l.observe(&clean(), &cfg(), 1_001);
        assert_eq!(l.state(), ClearanceState::EscalationRaised);
        assert!(l.is_immobilized(), "veto active in EscalationRaised");
        let now = 2_000u64;
        l.try_clear(&grant(now, 10), now, MAX_AGE).unwrap();
        assert!(!l.is_immobilized(), "released only after the grant");
    }

    /// Age boundary is INCLUSIVE: a grant exactly max_grant_age_ms old is
    /// well-formed; one ms older is not.
    #[test]
    fn test_grant_age_boundary_inclusive() {
        let now = 100_000u64;
        let exactly = OperatorClearanceGrant { operator_id: "op".into(), granted_at_ms: now - MAX_AGE };
        assert!(exactly.is_well_formed(now, MAX_AGE), "exactly max age is well-formed (inclusive)");
        let older = OperatorClearanceGrant { operator_id: "op".into(), granted_at_ms: now - (MAX_AGE + 1) };
        assert!(!older.is_well_formed(now, MAX_AGE), "one ms older is stale");
    }

    /// A future-dated grant is malformed (granted_at_ms > now).
    #[test]
    fn test_future_dated_grant_malformed() {
        let now = 100_000u64;
        let future = OperatorClearanceGrant { operator_id: "op".into(), granted_at_ms: now + 1 };
        assert!(!future.is_well_formed(now, MAX_AGE), "a future-dated grant must be malformed");
        // and is rejected by try_clear
        let mut l = escalated();
        assert_eq!(l.try_clear(&future, now, MAX_AGE), Err(ClearanceRejection::MalformedGrant));
        assert!(l.is_immobilized());
    }
}
