
import asyncio
import unittest
from unittest.mock import MagicMock, patch, AsyncMock
import time
import uuid

# Mocking modules before imports
with patch('heiwa_sdk.db.Database'), \
     patch('heiwa_sdk.spacetimedb.SpacetimeDB'), \
     patch('discord.ext.commands.Bot'), \
     patch('discord.Intents.default'):
    
    from apps.heiwa_hub.agents.messenger import MessengerAgent
    from heiwa_protocol.protocol import Subject
    from packages.heiwa_cognition.heiwa_cognition.intent import IntentProfile

async def verify_propagation():
    print("🚀 Starting verification of owner propagation...")
    
    # 1. Initialize MessengerAgent with mocked dependencies
    with patch('heiwa_sdk.db.Database'), \
         patch('heiwa_sdk.spacetimedb.SpacetimeDB'), \
         patch('discord.ext.commands.Bot'), \
         patch('apps.heiwa_hub.agents.messenger.LocalTaskPlanner') as MockPlannerClass:
        
        mock_planner = MockPlannerClass.return_value
        agent = MessengerAgent()
        
        # 2. Mock a Discord message from user "999"
        mock_author = MagicMock()
        mock_author.id = 999
        mock_author.__str__.return_value = "user-999"
        
        mock_channel = MagicMock()
        mock_channel.id = 12345
        
        mock_guild = MagicMock()
        
        mock_message = MagicMock()
        mock_message.author = mock_author
        mock_message.channel = mock_channel
        mock_message.guild = mock_guild
        mock_message.id = 67890
        
        # 3. Patch necessary methods
        agent.speak = AsyncMock()
        
        # Mock normalizer.normalize
        mock_intent_profile = IntentProfile(
            intent_class="build",
            risk_level="low",
            requires_approval=False,
            preferred_runtime="railway",
            preferred_tool="ollama",
            preferred_tier="tier1_local",
            brief="Mock brief",
            confidence=1.0,
            underspecified=False
        )
        mock_planner.normalizer.normalize.return_value = mock_intent_profile
        
        # Mock planner.plan to return a mock TaskPlan with to_dict method
        mock_plan = MagicMock()
        mock_plan.to_dict.return_value = {"status": "planned"}
        mock_planner.plan.return_value = mock_plan
        
        # 4. Trigger _ingest_interaction
        instruction = "Build a rocket"
        await agent._ingest_interaction(instruction, mock_message, explicit=True)
        
        # 5. Capture the published message on Subject.TASK_INGRESS
        # find the TASK_INGRESS publication
        ingress_payload = None
        for call in agent.speak.call_args_list:
            subject, payload = call.args
            if subject == Subject.TASK_INGRESS:
                ingress_payload = payload
                break
        
        if not ingress_payload:
            print("❌ Subject.TASK_INGRESS not found in agent.speak calls.")
            return False

        # 6. Verify ingress payload
        expected_owner_id = "discord-999"
        expected_principal_id = "discord-user-999"
        expected_session_prefix = "discord-session-999"
        
        print(f"Ingress Payload: {ingress_payload}")
        
        passed = True
        if ingress_payload["owner_id"] != expected_owner_id:
            print(f"❌ owner_id mismatch: expected {expected_owner_id}, got {ingress_payload['owner_id']}")
            passed = False
        else:
            print(f"✅ owner_id correctly propagated to ingress: {expected_owner_id}")
            
        if ingress_payload["principal_id"] != expected_principal_id:
            print(f"❌ principal_id mismatch: expected {expected_principal_id}, got {ingress_payload['principal_id']}")
            passed = False
        else:
            print(f"✅ principal_id correctly propagated to ingress: {expected_principal_id}")
            
        if not ingress_payload["session_id"].startswith(expected_session_prefix):
            print(f"❌ session_id mismatch: expected prefix {expected_session_prefix}, got {ingress_payload['session_id']}")
            passed = False
        else:
            print(f"✅ session_id correctly propagated to ingress: {ingress_payload['session_id']}")
            
        # 7. Verify planner.plan was called with these IDs
        mock_planner.plan.assert_called_once()
        plan_kwargs = mock_planner.plan.call_args.kwargs
        
        if plan_kwargs.get("owner_id") != expected_owner_id:
            print(f"❌ planner.plan owner_id mismatch: expected {expected_owner_id}, got {plan_kwargs.get('owner_id')}")
            passed = False
        else:
            print(f"✅ owner_id correctly passed to planner.plan")

        if plan_kwargs.get("principal_id") != expected_principal_id:
            print(f"❌ planner.plan principal_id mismatch: expected {expected_principal_id}, got {plan_kwargs.get('principal_id')}")
            passed = False
        else:
            print(f"✅ principal_id correctly passed to planner.plan")

        if not plan_kwargs.get("session_id", "").startswith(expected_session_prefix):
            print(f"❌ planner.plan session_id mismatch: expected prefix {expected_session_prefix}, got {plan_kwargs.get('session_id')}")
            passed = False
        else:
            print(f"✅ session_id correctly passed to planner.plan")
            
        return passed

if __name__ == "__main__":
    success = asyncio.run(verify_propagation())
    if success:
        print("\n✨ VERIFICATION PASSED")
        exit(0)
    else:
        print("\n❌ VERIFICATION FAILED")
        exit(1)
