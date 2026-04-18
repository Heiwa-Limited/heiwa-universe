import { beforeEach, describe, expect, it, vi } from "vitest";
import { POST as logoutPost } from "../../src/routes/auth/logout/+server";

const authMocks = vi.hoisted(() => ({
  assertMutableRequestOrigin: vi.fn(),
  parseSessionCookieValue: vi.fn(),
  requireSessionCookiePassword: vi.fn(),
  buildLogoutUrl: vi.fn(),
}));

vi.mock("../../src/lib/server/auth/session", () => ({
  SESSION_COOKIE_NAME: "heiwa_session",
  assertMutableRequestOrigin: authMocks.assertMutableRequestOrigin,
  parseSessionCookieValue: authMocks.parseSessionCookieValue,
  requireSessionCookiePassword: authMocks.requireSessionCookiePassword,
}));

vi.mock("../../src/lib/server/auth/workos", () => ({
  buildLogoutUrl: authMocks.buildLogoutUrl,
}));

describe("logout route", () => {
  beforeEach(() => {
    authMocks.assertMutableRequestOrigin.mockReset();
    authMocks.parseSessionCookieValue.mockReset();
    authMocks.requireSessionCookiePassword.mockReset();
    authMocks.buildLogoutUrl.mockReset();
    authMocks.requireSessionCookiePassword.mockReturnValue("test-cookie-password");
  });

  it("rejects bad origin on logout post", async () => {
    authMocks.assertMutableRequestOrigin.mockImplementation(() => {
      throw new Error("bad origin");
    });

    const response = await logoutPost({
      request: new Request("http://localhost:5173/auth/logout", {
        method: "POST",
        headers: {
          origin: "https://evil.example",
        },
      }),
      cookies: {
        get: vi.fn(() => "session-cookie"),
      },
    } as never);

    expect(response.status).toBe(403);
    expect(authMocks.parseSessionCookieValue).not.toHaveBeenCalled();
  });

  it("clears the session and redirects on protected post logout", async () => {
    authMocks.assertMutableRequestOrigin.mockReturnValue(undefined);
    authMocks.parseSessionCookieValue.mockReturnValue({
      sessionId: "session_123",
      userId: "user_123",
      email: "founder@example.com",
      issuedAt: 1713463200000,
      authAt: 1713463200000,
      lastSeenAt: 1713463200000,
      absoluteExpiresAt: 1713506400000,
    });
    authMocks.buildLogoutUrl.mockReturnValue("https://workos.example/logout");

    const deleteCookie = vi.fn();
    const response = await logoutPost({
      request: new Request("http://localhost:5173/auth/logout", {
        method: "POST",
        headers: {
          origin: "http://localhost:5173",
        },
      }),
      cookies: {
        get: vi.fn(() => "session-cookie"),
        delete: deleteCookie,
      },
    } as never);

    expect(response.status).toBe(303);
    expect(response.headers.get("location")).toBe("https://workos.example/logout");
    expect(deleteCookie).toHaveBeenCalledWith("heiwa_session", { path: "/" });
  });
});
