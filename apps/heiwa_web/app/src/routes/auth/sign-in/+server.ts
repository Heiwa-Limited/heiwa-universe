import type { RequestHandler } from "@sveltejs/kit";
import { shouldUseSecureCookies } from "../../../lib/server/auth/session";
import {
  AUTH_STATE_COOKIE_NAME,
  AUTH_STATE_MAX_AGE_MS,
  buildSignInUrl,
  issueAuthStateToken,
} from "../../../lib/server/auth/workos";

export const GET: RequestHandler = async ({ cookies, request }) => {
  const url = new URL(request.url);
  const secure = shouldUseSecureCookies(url);
  const state = issueAuthStateToken();

  cookies.set(AUTH_STATE_COOKIE_NAME, state, {
    httpOnly: true,
    path: "/auth",
    sameSite: "lax",
    secure,
    maxAge: Math.floor(AUTH_STATE_MAX_AGE_MS / 1000),
  });

  return new Response(null, {
    status: 303,
    headers: {
      location: buildSignInUrl(state),
      "cache-control": "no-store",
    },
  });
};
