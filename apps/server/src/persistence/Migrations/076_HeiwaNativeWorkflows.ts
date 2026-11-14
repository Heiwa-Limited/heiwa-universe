import * as Effect from "effect/Effect";
import * as SqlClient from "effect/sql/SqlClient";

export default Effect.gen(function* () {
  const sql = yield* SqlClient.SqlClient;
  yield* sql`CREATE TABLE heiwa_native_grants (app TEXT PRIMARY KEY, resource_ids_json TEXT NOT NULL)`;
  yield* sql`CREATE TABLE heiwa_native_proposals (
    id TEXT PRIMARY KEY, action_json TEXT NOT NULL, expected_json TEXT,
    status TEXT NOT NULL, created_at TEXT NOT NULL, result_json TEXT, detail TEXT
  )`;
  yield* sql`CREATE INDEX heiwa_native_proposals_status ON heiwa_native_proposals(status, created_at)`;
});
