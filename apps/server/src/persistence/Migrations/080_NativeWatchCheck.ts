import * as Effect from "effect/Effect";
import * as SqlClient from "effect/sql/SqlClient";
export default Effect.gen(function* () {
  const sql = yield* SqlClient.SqlClient;
  yield* sql`CREATE TABLE heiwa_native_watch (app TEXT PRIMARY KEY, checked_at TEXT NOT NULL, outcome TEXT NOT NULL, detail TEXT)`;
});
