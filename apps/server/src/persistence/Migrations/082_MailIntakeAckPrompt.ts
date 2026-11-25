import * as Effect from "effect/Effect";
import * as SqlClient from "effect/sql/SqlClient";

// Intake now holds each batch until the chat acknowledges it. A watch saved with the earlier
// prompt never acknowledges, so its batches would come back every 30 minutes. Only the exact
// stock prompt is replaced: a prompt the owner edited is theirs to update.
const BEFORE =
  "Run mail intake once. If it returns no messages, reply 'No new mail' and stop without calling another tool. Otherwise, for each returned message that asks to meet, stage one calendar event in one of my selected calendars, then stop. Stage each event only once.";
const AFTER =
  "Run mail intake once. If it returns no messages, reply 'No new mail' and stop without calling another tool. Otherwise, for each returned message that asks to meet, stage one calendar event in one of my selected calendars. If the batch is marked redelivered, skip any message whose event is already staged. Stage each event only once. When every message is handled, run mail intake with ack set to true, then stop.";

export default Effect.gen(function* () {
  const sql = yield* SqlClient.SqlClient;
  yield* sql`UPDATE scheduled_tasks SET prompt = ${AFTER} WHERE task_id = 'heiwa:mail-intake' AND prompt = ${BEFORE}`;
});
