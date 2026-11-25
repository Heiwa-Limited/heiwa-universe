import * as Effect from "effect/Effect";
import * as SqlClient from "effect/sql/SqlClient";
export default Effect.gen(function* () {
  const sql = yield* SqlClient.SqlClient;
  // Mail ROWID within the cursor's second. NULL on rows saved before: they resume at the second.
  yield* sql`ALTER TABLE heiwa_native_intake ADD COLUMN after_id INTEGER`;
  // The batch handed to one chat and not yet acknowledged: its end, holder and hold expiry.
  // `after`/`after_id` stay the committed position until the holder acknowledges.
  yield* sql`ALTER TABLE heiwa_native_intake ADD COLUMN claim_after TEXT`;
  yield* sql`ALTER TABLE heiwa_native_intake ADD COLUMN claim_after_id INTEGER`;
  yield* sql`ALTER TABLE heiwa_native_intake ADD COLUMN claim_holder TEXT`;
  yield* sql`ALTER TABLE heiwa_native_intake ADD COLUMN claim_until TEXT`;
});
