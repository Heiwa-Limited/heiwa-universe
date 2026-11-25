import * as Effect from "effect/Effect";
import * as SqlClient from "effect/sql/SqlClient";
export default Effect.gen(function* () {
  const sql = yield* SqlClient.SqlClient;
  // Mail ROWID within the cursor's second. NULL on rows saved before: they resume at the second.
  yield* sql`ALTER TABLE heiwa_native_intake ADD COLUMN after_id INTEGER`;
});
