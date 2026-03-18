# Acceptance Checklist

- The diagram shows STDB as the authority for proposal, approval, and lease state.
- The diagram includes `capability_leases`, `approval_requests`, and `approval_decisions`.
- The HTTP proposal surface is shown as stable public interface, not direct SQL.
- `tick.py` is shown as transitional scheduler, not state authority.
- Execution is visually downstream of an issued lease.
- `approval.py` and Discord are visually marked as transitional / compatibility layers.
- No frame implies proposal lifecycle subscriptions are already live.
