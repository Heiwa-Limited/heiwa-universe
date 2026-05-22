import os
import unittest
from unittest.mock import MagicMock, patch
from pathlib import Path
import sys

# Add project root and package directories to sys.path
project_root = Path(__file__).resolve().parents[1]
for pkg in (project_root / "packages").iterdir():
    if pkg.is_dir():
        sys.path.insert(0, str(pkg))

# Set HEIWA_MASTER_KEY for InstanceVault initialization
os.environ["HEIWA_MASTER_KEY"] = "dummy-master-key-at-least-32-chars-long-12345"

from heiwa_sdk.vault import UserVault, InstanceVault
from heiwa_cognition.router import ComputeRouter
from heiwa_sdk.heiwaclaw.gateway import OpenClaw
from heiwa_protocol.routing import BrokerRouteResult

class TestBYOKRouting(unittest.TestCase):
    def setUp(self):
        self.mock_stdb = MagicMock()
        # Mock escape_sql_literal
        self.mock_stdb._escape_sql_literal.side_effect = lambda s: str(s).replace("'", "''")
        
        # In-memory storage for credentials
        self.credentials = []
        
        def mock_update_provider_credential(credential_id, user_id, provider_id, credential_kind, credential_enc, rate_group, display_label=None):
            self.credentials.append({
                "credential_id": credential_id,
                "user_id": user_id,
                "provider_id": provider_id,
                "credential_kind": credential_kind,
                "credential_enc": credential_enc,
                "rate_group": rate_group,
                "status": "active"
            })
            return True
        
        def mock_get_provider_credentials(user_id):
            return [c for c in self.credentials if c["user_id"] == user_id and c["status"] == "active"]
        
        def mock_query(sql):
            if "SELECT * FROM provider_credentials" in sql:
                import re
                user_match = re.search(r"user_id = '([^']+)'", sql)
                provider_match = re.search(r"provider_id = '([^']+)'", sql)
                if user_match and provider_match:
                    user_id = user_match.group(1)
                    provider_id = provider_match.group(1)
                    return [c for c in self.credentials if c["user_id"] == user_id and c["provider_id"] == provider_id and c["status"] == "active"]
                elif user_match:
                    user_id = user_match.group(1)
                    return [c for c in self.credentials if c["user_id"] == user_id and c["status"] == "active"]
            return []

        self.mock_stdb.update_provider_credential.side_effect = mock_update_provider_credential
        self.mock_stdb.get_provider_credentials.side_effect = mock_get_provider_credentials
        self.mock_stdb.query.side_effect = mock_query
        
        # Mock model tiers
        self.mock_stdb.get_model_tiers.return_value = [
            {
                "model_id": "google/gemini-2.0-flash",
                "provider_model_id": "gemini-2.0-flash",
                "provider": "google",
                "rate_group": "google",
                "capability_class": 2,
                "effort_knob": "default",
                "effort_level": 1,
                "cost_per_turn": 0.01,
                "max_context_tokens": 32000,
                "strengths_json": '["general"]',
                "enabled": True
            }
        ]
        self.mock_stdb.get_gpu_slots.return_value = []
        self.mock_stdb.get_pods.return_value = []

        self.vault = UserVault(self.mock_stdb)
        self.router = ComputeRouter(stdb=self.mock_stdb)
        self.openclaw = OpenClaw(root_dir=project_root, stdb=self.mock_stdb)

    def test_byok_flow(self):
        owner_id = "user-123"
        provider_id = "google"
        api_key = "sk-dummy-google-key"

        # 1. Store a dummy credential
        print(f"\n1. Storing credential for {owner_id} / {provider_id}...")
        self.vault.store_credential(
            owner_id=owner_id,
            provider_id=provider_id,
            credential_kind="api_key",
            plaintext_value=api_key,
            rate_group="google",
            display_label="My Google Key"
        )
        
        # Verify it's stored in mock stdb
        creds = self.mock_stdb.get_provider_credentials(owner_id)
        self.assertEqual(len(creds), 1)
        self.assertEqual(creds[0]["provider_id"], provider_id)
        print("✅ Credential stored successfully.")

        # 2. Verify ComputeRouter.route_inference(owner_id="user-123") includes "google"
        print(f"2. Checking routing for {owner_id}...")
        plan = self.router.route_inference(intent="chat", risk="low", owner_id=owner_id)
        available_providers = {plan.primary.provider} | {f.provider for f in plan.fallbacks}
        print(f"   Available providers for {owner_id}: {available_providers}")
        self.assertIn("google", available_providers)
        print("✅ 'google' provider is available for user-123.")

        # 3. Verify ComputeRouter.route_inference(owner_id="user-456") does NOT include "google"
        other_owner = "user-456"
        print(f"3. Checking routing for {other_owner}...")
        plan_other = self.router.route_inference(intent="chat", risk="low", owner_id=other_owner)
        available_providers_other = {plan_other.primary.provider} | {f.provider for f in plan_other.fallbacks}
        print(f"   Available providers for {other_owner}: {available_providers_other}")
        self.assertNotIn("google", available_providers_other)
        print("✅ 'google' provider is NOT available for user-456.")

        # 4. Verify OpenClaw.resolve(owner_id="user-123") injects GEMINI_API_KEY
        print(f"4. Resolving route for {owner_id} on 'google' provider...")
        # Create a dummy route result using from_payload to handle required fields
        route_result = BrokerRouteResult.from_payload({
            "task_id": "task-1",
            "assigned_worker": "google-worker",
            "target_tool": "heiwa_claw",
            "target_model": "google/gemini-2.0-flash",
            "target_runtime": "macbook",
            "target_tier": "tier1",
            "compute_class": 2,
            "rationale": "Test",
            "requires_approval": False
        })
        
        # We need to make sure _provider_for returns "google"
        with patch.object(self.openclaw, '_provider_for', return_value="google"):
            dispatch = self.openclaw.resolve(route_result, owner_id=owner_id)
            
            print(f"   Adapter env keys: {list(dispatch.adapter_env.keys())}")
            self.assertIn("GEMINI_API_KEY", dispatch.adapter_env)
            self.assertEqual(dispatch.adapter_env["GEMINI_API_KEY"], api_key)
            print("✅ GEMINI_API_KEY injected successfully for user-123.")

if __name__ == "__main__":
    unittest.main()
