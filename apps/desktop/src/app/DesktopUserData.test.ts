import * as NodeServices from "@effect/platform-node/NodeServices";
import { assert, it } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Path from "effect/Path";
import { resolveUserDataPath } from "./DesktopUserData.ts";

it.effect.each(["darwin", "win32", "linux"] as const)(
  "isolates Heiwa browser credentials from T3 profiles on %s",
  (platform) =>
    Effect.gen(function* () {
      const fs = yield* FileSystem.FileSystem;
      const path = yield* Path.Path;
      const directory = yield* fs.makeTempDirectoryScoped({ prefix: "heiwa-profile-" });
      for (const source of ["t3code", "t3code-v2", "T3 Code (Alpha)", "T3 Code (Dev)"]) {
        yield* fs.makeDirectory(path.join(directory, source));
        yield* fs.writeFileString(
          path.join(directory, source, "Local State"),
          "upstream credentials",
        );
      }
      for (const isDevelopment of [false, true]) {
        const result = yield* resolveUserDataPath({
          appDataDirectory: directory,
          baseDir: path.join(directory, ".heiwa"),
          homeDirectory: directory,
          isDevelopment,
          platform,
        });
        assert.equal(result, path.join(directory, isDevelopment ? "Heiwa Dev" : "Heiwa"));
        assert.isFalse(yield* fs.exists(path.join(result, "Local State")));
        const isolated = path.join(directory, "isolated-profile");
        assert.equal(
          yield* resolveUserDataPath({
            appDataDirectory: directory,
            isDevelopment,
            platform,
            baseDir: isolated,
            homeDirectory: directory,
          }),
          path.join(isolated, "electron", isDevelopment ? "development" : "desktop"),
        );
      }
    }).pipe(Effect.scoped, Effect.provide(NodeServices.layer)),
);
