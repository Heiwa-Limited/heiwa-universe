import type { RequestHandler } from "@sveltejs/kit";
import { SESSION_COOKIE_NAME, shouldUseSecureCookies } from "../../../lib/server/auth/session";
import {
  AUTH_STATE_COOKIE_NAME,
  assertAuthState,
  exchangeCallback,
} from "../../../lib/server/auth/workos";

export const GET: RequestHandler = async ({ cookies, request }) => {
  const url = new URL(request.url);
  const secure = shouldUseSecureCookies(url);

  try {
    assertAuthState(url.searchParams.get("state"), cookies.get(AUTH_STATE_COOKIE_NAME) ?? undefined);

    const now = Date.now();
    const { cookieValue, session } = await exchangeCallback(request, now);

    cookies.set(SESSION_COOKIE_NAME, cookieValue, {
      httpOnly: true,
      path: "/",
      sameSite: "lax",
      secure,
      maxAge: Math.max(0, Math.floor((session.absoluteExpiresAt - now) / 1000)),
    });
    cookies.delete(AUTH_STATE_COOKIE_NAME, { path: "/auth" });

    return new Response(null, {
      status: 303,
      headers: {
        location: "/app",
        "cache-control": "no-store",
      },
    });
  } catch {
    cookies.delete(AUTH_STATE_COOKIE_NAME, { path: "/auth" });

    return new Response(null, {
      status: 303,
      headers: {
        location: "/app?auth=failed",
        "cache-control": "no-store",
      },
    });
  }
};
