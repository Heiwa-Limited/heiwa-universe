import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SESSION_COOKIE_NAME } from "../../src/lib/server/auth/session";
import { load as appLayoutLoad } from "../../src/routes/app/+layout.server";
import { GET as callbackGet } from "../../src/routes/auth/callback/+server";

const authMocks = vi.hoisted(() => ({
  exchangeCallback: vi.fn(),
  assertAuthState: vi.fn(),
}));

vi.mock("../../src/lib/server/auth/workos", () => ({
  AUTH_STATE_COOKIE_NAME: "heiwa_auth_state",
  AUTH_STATE_MAX_AGE_MS: 15 * 60 * 1000,
  assertAuthState: authMocks.assertAuthState,
  exchangeCallback: authMocks.exchangeCallback,
}));

describe("auth callback route", () => {
  beforeEach(() => {
    authMocks.exchangeCallback.mockReset();
    authMocks.assertAuthState.mockReset();
    authMocks.assertAuthState.mockImplementation(() => undefined);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("issues a session cookie after callback when state matches", async () => {
    const now = Date.UTC(2026, 3, 18, 18, 0, 0);
    const session = {
      sessionId: "session_123",
      userId: "user_123",
      email: "founder@example.com",
      organizationId: "org_123",
      issuedAt: now,
      authAt: now,
      lastSeenAt: now,
      absoluteExpiresAt: now + 45 * 60 * 1000,
    };

    authMocks.exchangeCallback.mockResolvedValue({
      session,
      cookieValue: "signed-cookie-value",
    });
    vi.spyOn(Date, "now").mockReturnValue(now);

    const set = vi.fn();
    const deleteCookie = vi.fn();
    const request = new Request("https://app.heiwa.ltd/auth/callback?code=workos_code&state=browser-state", {
      method: "GET",
    });

    const response = await callbackGet({
      url: new URL(request.url),
      request,
      cookies: {
        get: vi.fn(() => "browser-state"),
        set,
        delete: deleteCookie,
      },
    } as never);

    expect(authMocks.assertAuthState).toHaveBeenCalledWith("browser-state", "browser-state");
    expect(authMocks.exchangeCallback).toHaveBeenCalledTimes(1);
    expect(set).toHaveBeenCalledWith(
      SESSION_COOKIE_NAME,
      "signed-cookie-value",
      expect.objectContaining({
        httpOnly: true,
        path: "/",
        sameSite: "lax",
        secure: true,
        maxAge: 45 * 60,
      })
    );
    expect(deleteCookie).toHaveBeenCalledWith("heiwa_auth_state", { path: "/auth" });
    expect(response.status).toBe(303);
    expect(response.headers.get("location")).toBe("/app");
  });

  it("redirects to failure when state verification fails", async () => {
    authMocks.assertAuthState.mockImplementation(() => {
      throw new Error("bad auth state");
    });

    const set = vi.fn();
    const deleteCookie = vi.fn();
    const request = new Request("https://app.heiwa.ltd/auth/callback?code=workos_code&state=browser-state", {
      method: "GET",
    });

    const response = await callbackGet({
      url: new URL(request.url),
      request,
      cookies: {
        get: vi.fn(() => undefined),
        set,
        delete: deleteCookie,
      },
    } as never);

    expect(authMocks.assertAuthState).toHaveBeenCalledWith("browser-state", undefined);
    expect(authMocks.exchangeCallback).not.toHaveBeenCalled();
    expect(set).not.toHaveBeenCalled();
    expect(deleteCookie).toHaveBeenCalledWith("heiwa_auth_state", { path: "/auth" });
    expect(response.status).toBe(303);
    expect(response.headers.get("location")).toBe("/app?auth=failed");
  });

  it("redirects to failure when the auth code is missing", async () => {
    const request = new Request("https://app.heiwa.ltd/auth/callback?state=browser-state", {
      method: "GET",
    });

    const response = await callbackGet({
      url: new URL(request.url),
      request,
      cookies: {
        get: vi.fn(() => "browser-state"),
        set: vi.fn(),
        delete: vi.fn(),
      },
    } as never);

    expect(authMocks.assertAuthState).toHaveBeenCalledWith("browser-state", "browser-state");
    expect(authMocks.exchangeCallback).toHaveBeenCalledTimes(1);
    expect(response.status).toBe(303);
    expect(response.headers.get("location")).toBe("/app?auth=failed");
  });

  it("redirects unauthenticated requests away from /app unless it is the failure landing page", async () => {
    await expect(
      appLayoutLoad({
        locals: {
          auth: null,
        },
        url: new URL("https://app.heiwa.ltd/app"),
      } as never)
    ).rejects.toMatchObject({
      status: 303,
      location: "/auth/sign-in",
    });

    await expect(
      appLayoutLoad({
        locals: {
          auth: null,
        },
        url: new URL("https://app.heiwa.ltd/app?auth=failed"),
      } as never)
    ).resolves.toMatchObject({
      auth: null,
      authFailed: true,
    });
  });
});
