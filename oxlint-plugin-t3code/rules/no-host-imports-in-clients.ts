// @effect-diagnostics nodeBuiltinImport:off -- Oxlint visitors synchronously resolve import paths without an Effect runtime.
import { defineRule, type ESTree } from "@oxlint/plugins";
import * as NodeModule from "node:module";
import * as NodePath from "node:path";

const CLIENT_SOURCE = /\/(apps\/(?:web|mobile)|packages\/(?:client-runtime|contracts))\/src\//u;
const TEST_SOURCE =
  /(?:\.(?:test|spec|test-support)\.[cm]?[jt]sx?$|\/(?:test|tests|testing|testUtils|test-support|fixtures)\/)/u;
const HOST_PACKAGE = /^(?:electron|@effect\/platform-node|@t3tools\/shared\/HostProcess)(?:\/|$)/u;
const APP_PACKAGE = /^(?:t3|@t3tools\/(?:server|desktop|web|mobile))(?:\/|$)/u;
const MESSAGE =
  "Client and contract modules must remain portable across web, desktop and mobile. Put host operations in a server service or the desktop bridge; share wire types through @t3tools/contracts and client logic through @t3tools/client-runtime.";

export default defineRule({
  meta: {
    type: "problem",
    docs: {
      description:
        "Keep host runtimes and application internals out of portable clients and contracts.",
    },
  },
  create(context) {
    const filename = context.filename.replaceAll("\\", "/");
    const match = CLIENT_SOURCE.exec(filename);
    if (!match || TEST_SOURCE.test(filename)) return {};
    const root = filename.slice(0, match.index);
    const owner = match[1];

    const check = (source: ESTree.Node | null, typeOnly = false) => {
      if (source === null) return;
      const value =
        source.type === "Literal"
          ? source.value
          : source.type === "TemplateLiteral" && source.expressions.length === 0
            ? source.quasis[0]?.value.cooked
            : undefined;
      if (typeof value !== "string") return;
      const target = value.startsWith(".")
        ? NodePath.posix.resolve(NodePath.posix.dirname(filename), value)
        : value;
      const app = target.startsWith(`${root}/apps/`)
        ? target
            .slice(root.length + 1)
            .split("/")
            .slice(0, 2)
            .join("/")
        : undefined;
      const hostRuntime = !typeOnly && (NodeModule.isBuiltin(value) || HOST_PACKAGE.test(value));
      const appInternals = APP_PACKAGE.test(value) || (app !== undefined && app !== owner);
      const contractsImportClient =
        owner === "packages/contracts" &&
        (/^@t3tools\/client-runtime(?:\/|$)/u.test(value) ||
          target.startsWith(`${root}/packages/client-runtime/`));
      if (hostRuntime || appInternals || contractsImportClient) {
        context.report({ node: source, message: MESSAGE });
      }
    };

    return {
      ImportDeclaration(node) {
        check(
          node.source,
          node.importKind === "type" ||
            (node.specifiers.length > 0 &&
              node.specifiers.every(
                (specifier) =>
                  specifier.type === "ImportSpecifier" && specifier.importKind === "type",
              )),
        );
      },
      ExportNamedDeclaration(node) {
        check(
          node.source,
          node.exportKind === "type" ||
            (node.specifiers.length > 0 &&
              node.specifiers.every((specifier) => specifier.exportKind === "type")),
        );
      },
      ExportAllDeclaration: (node) => check(node.source, node.exportKind === "type"),
      ImportExpression: (node) => check(node.source),
      TSExternalModuleReference: (node) => check(node.expression),
      CallExpression(node) {
        if (node.callee.type === "Identifier" && node.callee.name === "require") {
          check(node.arguments[0] ?? null);
        }
      },
    };
  },
});
