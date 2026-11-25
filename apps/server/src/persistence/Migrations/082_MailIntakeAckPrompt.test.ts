import { assert, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as SqlClient from "effect/sql/SqlClient";

import { runMigrations } from "../Migrations.ts";
import * as NodeSqliteClient from "@t3tools/shared/nodeSqliteClient";

const layer = it.layer(Layer.mergeAll(NodeSqliteClient.layer({ filename: ":memory:" })));

const STOCK =
  "Run mail intake once. If it returns no messages, reply 'No new mail' and stop without calling another tool. Otherwise, for each returned message that asks to meet, stage one calendar event in one of my selected calendars, then stop. Stage each event only once.";

layer("082_MailIntakeAckPrompt", (it) => {
  it.effect("upgrades only the stock intake prompt so saved watches acknowledge", () =>
    Effect.gen(function* () {
      const sql = yield* SqlClient.SqlClient;
      yield* runMigrations({ toMigrationInclusive: 81 });
      const task = (id: string, prompt: string) =>
        sql`INSERT INTO scheduled_tasks (task_id, title, prompt, enabled, schedule_json, project_id,
          workspace_strategy_json, model_selection_json, runtime_mode, interaction_mode, created_by,
          creation_source, created_at, updated_at, last_run_status, run_count)
          VALUES (${id}, 'Mail intake', ${prompt}, 1, '{}', 'p', '{}', '{}', 'approval-required',
          'default', 'user', 'web', '2026-11-25T00:00:00Z', '2026-11-25T00:00:00Z', 'never', 0)`;
      yield* task("heiwa:mail-intake", STOCK);
      yield* task("custom", STOCK);
      yield* runMigrations({ toMigrationInclusive: 82 });
      const rows = yield* sql<{ task_id: string; prompt: string }>`
        SELECT task_id, prompt FROM scheduled_tasks ORDER BY task_id`;
      const byId = Object.fromEntries(rows.map((row) => [row.task_id, row.prompt]));
      assert.include(byId["heiwa:mail-intake"], "run mail intake with ack set to true");
      // Other tasks keep their prompt, even one that happens to match the old text.
      assert.strictEqual(byId.custom, STOCK);
    }),
  );
});

// A separate database: the owner's edited prompt is in place before migration 82 runs.
it.layer(NodeSqliteClient.layer({ filename: ":memory:" }))("082 with an edited prompt", (it) => {
  it.effect("leaves an intake prompt the owner edited alone", () =>
    Effect.gen(function* () {
      const sql = yield* SqlClient.SqlClient;
      yield* runMigrations({ toMigrationInclusive: 81 });
      yield* sql`INSERT INTO scheduled_tasks (task_id, title, prompt, enabled, schedule_json, project_id,
        workspace_strategy_json, model_selection_json, runtime_mode, interaction_mode, created_by,
        creation_source, created_at, updated_at, last_run_status, run_count)
        VALUES ('heiwa:mail-intake', 'Mail intake', 'my own words', 1, '{}', 'p', '{}', '{}',
        'approval-required', 'default', 'user', 'web', '2026-11-25T00:00:00Z',
        '2026-11-25T00:00:00Z', 'never', 0)`;
      yield* runMigrations({ toMigrationInclusive: 82 });
      const [row] = yield* sql<{ prompt: string }>`
        SELECT prompt FROM scheduled_tasks WHERE task_id = 'heiwa:mail-intake'`;
      assert.strictEqual(row?.prompt, "my own words");
    }),
  );
});
