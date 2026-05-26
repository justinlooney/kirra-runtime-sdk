# Active Work

> Maximum 3 tasks in flight at once (WIP limit). Matches In Progress column on
> the project board. Pull from Ready when a slot opens; move to done.md on merge.
>
> **Authority model reminder (all active tasks):** KirraGovernor applies the MRC
> fallback profile (5.0 m/s ceiling) on Degraded and LockedOut — velocity cap,
> not a hard zero veto. Both postures share the same contract. Nominal uses the
> reference profile (35.0 m/s) with a stricter acceleration rate-limit. Falls back
> to the built-in 1.5 m/s clamp if governor is unreachable. Synchronous path:
> `planned_cmd → governor → final_cmd`.
>
> **Test count reminder:** parko-core has ~30–40 tests (NOT 333). kirra-runtime-sdk
> holds ~333 tests. Do not conflate the two.

---

