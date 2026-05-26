# Active Work

> Maximum 3 tasks in flight at once (WIP limit). Matches In Progress column on
> the project board. Pull from Ready when a slot opens; move to done.md on merge.
>
> **Authority model reminder (all active tasks):**
> - **LockedOut** → hard stop, 0.0 m/s (`EnforcementAction::Deny`), no motion
>   permitted, requires human intervention. NOT the MRC cap.
> - **Degraded** → MRC fallback profile, 5.0 m/s ceiling (velocity cap, not a
>   hard zero veto).
> - **Nominal** → nominal reference profile, 35.0 m/s ceiling + stricter
>   rate-of-change limit.
> LockedOut and Degraded are **separate branches** that must never share a code
> path or contract instance. Falls back to the built-in 1.5 m/s clamp if governor
> is unreachable. Synchronous path: `planned_cmd → governor → final_cmd`.
>
> **Test count reminder:** parko-core has ~30–40 tests (NOT 333). kirra-runtime-sdk
> holds ~333 tests. Do not conflate the two.

---

