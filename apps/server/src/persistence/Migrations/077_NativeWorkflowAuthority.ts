import * as Effect from "effect/Effect";
import * as SqlClient from "effect/sql/SqlClient";
export default Effect.gen(function* () {
  const sql = yield* SqlClient.SqlClient;
  yield* sql`ALTER TABLE heiwa_native_grants ADD COLUMN resources_json TEXT NOT NULL DEFAULT '[]'`;
  yield* sql`ALTER TABLE heiwa_native_grants ADD COLUMN write_mode TEXT NOT NULL DEFAULT 'review'`;
  yield* sql`ALTER TABLE heiwa_native_proposals ADD COLUMN resource_json TEXT`;
  yield* sql`ALTER TABLE heiwa_native_proposals ADD COLUMN origin TEXT NOT NULL DEFAULT 'owner'`;
});
