import type { Handle } from "@sveltejs/kit";
import {
  SESSION_COOKIE_NAME,
  createSessionCookieValue,
  shouldUseSecureCookies,
  parseSessionCookieValue,
  refreshSessionActivity,
  requireSessionCookiePassword,
} from "./lib/server/auth/session";

export const handle: Handle = async ({ event, resolve }) => {
  const cookiePassword = requireSessionCookiePassword(process.env.WORKOS_COOKIE_PASSWORD);
  const cookieValue = event.cookies.get(SESSION_COOKIE_NAME);
  const session = parseSessionCookieValue(cookieValue, cookiePassword);

  if (!session) {
    event.cookies.delete(SESSION_COOKIE_NAME, { path: "/" });
    event.locals.auth = null;
    return resolve(event);
  }

  if (event.url.pathname === "/auth/logout") {
    event.locals.auth = session;
    return resolve(event);
  }

  const now = Date.now();
  const refreshed = refreshSessionActivity(session, now);
  event.locals.auth = refreshed;
  event.cookies.set(SESSION_COOKIE_NAME, createSessionCookieValue(refreshed, cookiePassword), {
    httpOnly: true,
    path: "/",
    sameSite: "lax",
    secure: shouldUseSecureCookies(event.url),
    maxAge: Math.max(0, Math.floor((refreshed.absoluteExpiresAt - now) / 1000)),
  });

  return resolve(event);
};
