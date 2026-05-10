import unittest
from unittest.mock import MagicMock, patch
import datetime
import json
from heiwa_sdk.spacetimedb import SpacetimeDB

class TestSTDBDialectCleanup(unittest.TestCase):
    def setUp(self):
        self.stdb = SpacetimeDB(db_identity="test_db", server="test_server")

    @patch("heiwa_sdk.spacetimedb.SpacetimeDB._run")
    def test_get_routable_proposals_sql_dialect(self, mock_run):
        # Mock successful run returning empty table
        mock_run.return_value = MagicMock(
            returncode=0, 
            stdout="proposal_id | status | created_at\n---------+--------+-----------\n"
        )
        
        self.stdb.get_routable_proposals()
        
        # Capture the SQL query sent to _run
        # The command is ["spacetime", "sql", "--server", server, db_identity, sql]
        args, kwargs = mock_run.call_args
        cmd = args[0]
        sql = cmd[-1]
        
        # Verify IN clause is NOT present and OR is present
        self.assertNotIn("status IN ('APPROVED', 'QUEUED')", sql)
        self.assertIn("(status = 'APPROVED' OR status = 'QUEUED')", sql)

    @patch("heiwa_sdk.spacetimedb.SpacetimeDB._run")
    def test_retry_logic_mechanism(self, mock_run):
        # Setup mock to fail twice then succeed
        # In the real implementation, _run returns None when returncode != 0
        success_res = MagicMock(returncode=0, stdout="OK")
        
        mock_run.side_effect = [None, None, success_res]
        
        with patch("time.sleep"): # Don't actually wait during tests
            result = self.stdb.upsert_node_heartbeat(node_id="test-node")
            
        self.assertTrue(result)
        self.assertEqual(mock_run.call_count, 3)

if __name__ == "__main__":
    unittest.main()
