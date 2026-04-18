import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const authMocks = vi.hoisted(() => ({
  parseSessionCookieValue: vi.fn(),
  refreshSessionActivity: vi.fn(),
  createSessionCookieValue: vi.fn(),
  requireSessionCookiePassword: vi.fn(),
  shouldUseSecureCookies: vi.fn(),
}));

vi.mock("../../src/lib/server/auth/session", () => ({
  SESSION_COOKIE_NAME: "heiwa_session",
  createSessionCookieValue: authMocks.createSessionCookieValue,
  parseSessionCookieValue: authMocks.parseSessionCookieValue,
  refreshSessionActivity: authMocks.refreshSessionActivity,
  requireSessionCookiePassword: authMocks.requireSessionCookiePassword,
  shouldUseSecureCookies: authMocks.shouldUseSecureCookies,
}));

import { handle } from "../../src/hooks.server";

describe("hooks server auth refresh", () => {
  beforeEach(() => {
    authMocks.parseSessionCookieValue.mockReset();
    authMocks.refreshSessionActivity.mockReset();
    authMocks.createSessionCookieValue.mockReset();
    authMocks.requireSessionCookiePassword.mockReset();
    authMocks.shouldUseSecureCookies.mockReset();
    authMocks.requireSessionCookiePassword.mockReturnValue("test-cookie-password");
    authMocks.shouldUseSecureCookies.mockReturnValue(false);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("preserves maxAge when refreshing a session cookie", async () => {
    const now = Date.UTC(2026, 3, 18, 18, 0, 0);
    const session = {
      sessionId: "session_123",
      userId: "user_123",
      email: "founder@example.com",
      issuedAt: now - 1000,
      authAt: now - 1000,
      lastSeenAt: now - 1000,
      absoluteExpiresAt: now + 45 * 60 * 1000,
    };

    authMocks.parseSessionCookieValue.mockReturnValue(session);
    authMocks.refreshSessionActivity.mockReturnValue({
      ...session,
      lastSeenAt: now,
    });
    authMocks.createSessionCookieValue.mockReturnValue("refreshed-cookie-value");
    vi.spyOn(Date, "now").mockReturnValue(now);

    const set = vi.fn();
    const deleteCookie = vi.fn();
    const resolve = vi.fn(async () => new Response("ok"));

    await handle({
      event: {
        url: new URL("http://localhost:5173/app"),
        cookies: {
          get: vi.fn(() => "existing-cookie"),
          set,
          delete: deleteCookie,
        },
        locals: {},
      },
      resolve,
    } as never);

    expect(set).toHaveBeenCalledWith(
      "heiwa_session",
      "refreshed-cookie-value",
      expect.objectContaining({
        httpOnly: true,
        path: "/",
        sameSite: "lax",
        secure: false,
        maxAge: 45 * 60,
      })
    );
    expect(deleteCookie).not.toHaveBeenCalled();
  });

  it("does not refresh the session cookie on logout requests", async () => {
    const now = Date.UTC(2026, 3, 18, 18, 0, 0);
    const session = {
      sessionId: "session_123",
      userId: "user_123",
      email: "founder@example.com",
      issuedAt: now - 1000,
      authAt: now - 1000,
      lastSeenAt: now - 1000,
      absoluteExpiresAt: now + 45 * 60 * 1000,
    };

    authMocks.parseSessionCookieValue.mockReturnValue(session);
    vi.spyOn(Date, "now").mockReturnValue(now);

    const set = vi.fn();
    const deleteCookie = vi.fn();
    const resolve = vi.fn(async () => new Response("ok"));

    await handle({
      event: {
        url: new URL("http://localhost:5173/auth/logout"),
        cookies: {
          get: vi.fn(() => "existing-cookie"),
          set,
          delete: deleteCookie,
        },
        locals: {},
      },
      resolve,
    } as never);

    expect(set).not.toHaveBeenCalled();
    expect(deleteCookie).not.toHaveBeenCalled();
    expect(resolve).toHaveBeenCalledTimes(1);
  });
});
