import asyncio
import os
from datetime import datetime
from typing import List

from textual.app import App, ComposeResult
from textual.containers import Container, Grid
from textual.widgets import Header, Footer, Static, RichLog, Label, Button
from textual.coordinate import Coordinate
from textual.reactive import reactive

class TaskEventLog(RichLog):
    """Logs the last 10 task events dynamically."""
    def add_event(self, msg: str):
        timestamp = datetime.now().strftime("%H:%M:%S")
        self.write(f"[{timestamp}] {msg}")

class ControlPanelWidget(Static):
    """Grid container for active control plane routes and metrics."""
    def compose(self) -> ComposeResult:
        with Grid(id="dashboard-grid"):
            yield Label("🟢 ACTIVE ROUTE: ", id="active-route-title")
            yield Label("N/A", id="active-route-value")
            
            yield Label("🔒 LEASE STATUS: ", id="lease-status-title")
            yield Label("No Active Lease", id="lease-status-value")
            
            yield Label("👑 APPROVAL STATE:", id="approval-state-title")
            yield Label("Idle", id="approval-state-value")

class OverlayDialog(Static):
    """Overlay dialog component frame framework."""
    def compose(self) -> ComposeResult:
         yield Label("⚠️ Approvals Action Required", id="overlay-title")
         yield Button("Approve", variant="success")
         yield Button("Deny", variant="error")

class HeiwaDashboardApp(App):
    """Textual TUI local operator dashboard dashboard for Heiwa Swarm."""

    TITLE = "HEIWA CONTROL PLANE"
    SUB_TITLE = "Local Operator Surface v1.0"
    
    CSS = """
    #dashboard-grid {
        grid-size: 2;
        grid-columns: 1fr 2fr;
        padding: 1;
        background: $boost;
        border: solid $primary;
    }
    #active-route-title, #lease-status-title, #approval-state-title {
        color: $accent;
        text-style: bold;
    }
    #active-route-value, #lease-status-value, #approval-state-value {
        color: $text;
    }
    #logs {
        height: 1fr;
        border: solid $primary;
        padding: 1;
    }
    #overlay-title {
         text-align: center;
         text-style: bold;
         padding: 1;
    }
    OverlayDialog {
         background: $boost;
         border: thick $warning;
         padding: 1 2;
         margin: 2 4;
         height: auto;
         dock: top;
    }
    """

    BINDINGS = [
        ("a", "toggle_approval", "Toggle Approvals (Demo)"),
        ("q", "quit", "Quit"),
    ]

    active_route = reactive("Idle (Waiting for dispatch)")
    lease_status = reactive("Uncapped (Observe Mode)")
    approval_state = reactive("Waiting for Human Approvals")

    def compose(self) -> ComposeResult:
        yield Header()
        yield Container(
            ControlPanelWidget(),
            TaskEventLog(id="logs"),
        )
        yield Footer()

    def on_mount(self) -> None:
        self.log_widget = self.query_one("#logs", TaskEventLog)
        self.log_widget.add_event("Dashboard mounted successfully.")
        self.log_widget.add_event(f"Node: {os.getenv('HEIWA_NODE_ID', 'local_operator')}")
        self.set_interval(3.0, self.update_dashboard)

    async def update_dashboard(self) -> None:
        """Query state layers dynamically via STDB bridge."""
        try:
            from heiwa_sdk.spacetimedb import SpacetimeDB
            import os
            db = SpacetimeDB(
                db_identity=os.environ.get("STDB_IDENTITY", "heiwaproductiondb"),
                server=os.environ.get("STDB_SERVER", "local")
            )
            
            def fetch_state():
                return (
                    db.query("SELECT * FROM route_decisions ORDER BY created_at DESC LIMIT 1"),
                    db.query("SELECT * FROM capability_leases ORDER BY issued_at DESC LIMIT 1"),
                    db.query("SELECT * FROM approval_requests ORDER BY requested_at DESC LIMIT 1")
                )

            routes, leases, approvals = await asyncio.to_thread(fetch_state)

            if routes:
                route = routes[0]
                self.active_route = f"{route.get('target_tool', 'Unknown')} -> {route.get('assigned_worker', 'Unknown')}"
            
            if leases:
                lease = leases[0]
                self.lease_status = f"{lease.get('status', 'Unknown')} ({lease.get('holder_id', 'Unknown')})"
                
            if approvals:
                appr = approvals[0]
                self.approval_state = f"{appr.get('status', 'Unknown')} (for {appr.get('proposal_id', 'Unknown')})"
                
        except Exception as e:
            pass # Keep silent if db fails during observe mode dev cycles.

        # Read from self.active_route reactive bindings setup triggers
        self.query_one("#active-route-value", Label).update(self.active_route)
        self.query_one("#lease-status-value", Label).update(self.lease_status)
        self.query_one("#approval-state-value", Label).update(self.approval_state)

    def action_toggle_approval(self) -> None:
        """Trigger bound to 'a' coordinate to toggle the approval overlay."""
        if self.query(OverlayDialog):
            for dialog in self.query(OverlayDialog):
                dialog.remove()
            self.trigger_task_event("Dismissed Approval Dialog")
        else:
            self.mount(OverlayDialog())
            self.trigger_task_event("Triggered Approval Dialog")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle dialog buttons."""
        if event.button.variant == "success":
            self.trigger_task_event("Approval Granted via Dashboard")
            if self.query(OverlayDialog):
                self.query(OverlayDialog).remove()
        elif event.button.variant == "error":
            self.trigger_task_event("Approval Denied via Dashboard")
            if self.query(OverlayDialog):
                self.query(OverlayDialog).remove()

    def trigger_task_event(self, text: str):
        self.query_one("#logs", TaskEventLog).add_event(text)

if __name__ == "__main__":
    app = HeiwaDashboardApp()
    app.run()
