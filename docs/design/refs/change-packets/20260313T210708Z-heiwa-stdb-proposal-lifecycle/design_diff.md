# Design Diff

## Add

- A control-plane state cluster inside the STDB node:
  - `proposals`
  - `proposal_consents`
  - `approval_requests`
  - `approval_decisions`
  - `capability_leases`
- A visible write path from:
  - hub HTTP `/proposals*`
  - router tick assignment
  - node heartbeat / claim flow
    into STDB
- A lease artifact between proposal claim and execution

## Change

- Move “assignment / claim / consent / lease” ownership from compatibility SQL to STDB
- Show FastAPI endpoints as stable interface wrappers rather than direct SQL handlers
- Show `tick.py` as a scheduler writing into STDB, not as the authoritative state machine

## Keep Transitional

- `approval.py` in-memory registry
- Discord / RFC notification surfaces
- status-only websocket stream
