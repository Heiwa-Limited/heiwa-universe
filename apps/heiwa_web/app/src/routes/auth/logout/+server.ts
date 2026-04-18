import type { RequestHandler } from "@sveltejs/kit";
import {
  assertMutableRequestOrigin,
  SESSION_COOKIE_NAME,
  parseSessionCookieValue,
  requireSessionCookiePassword,
} from "../../../lib/server/auth/session";
import { buildLogoutUrl } from "../../../lib/server/auth/workos";

export const POST: RequestHandler = async ({ cookies, request }) => {
  const requestOrigin = new URL(request.url).origin;

  try {
    assertMutableRequestOrigin(request, requestOrigin);
  } catch {
    return new Response("Forbidden", { status: 403 });
  }

  const cookiePassword = requireSessionCookiePassword(process.env.WORKOS_COOKIE_PASSWORD);

  const session = parseSessionCookieValue(cookies.get(SESSION_COOKIE_NAME) ?? undefined, cookiePassword);
  const redirectTo = session ? buildLogoutUrl(session, `${requestOrigin}/app`) : "/app";
  cookies.delete(SESSION_COOKIE_NAME, { path: "/" });

  return new Response(null, {
    status: 303,
    headers: {
      location: redirectTo,
      "cache-control": "no-store",
    },
  });
};
