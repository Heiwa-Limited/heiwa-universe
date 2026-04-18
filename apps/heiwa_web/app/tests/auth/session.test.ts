import { describe, expect, it } from "vitest";
import {
  assertCsrfToken,
  SESSION_COOKIE_NAME,
  buildCsrfCookieHeader,
  shouldUseSecureCookies,
  assertMutableRequestOrigin,
  assertRecentAuth,
  createSessionCookieValue,
  issueCsrfToken,
  parseSessionCookieValue,
  sessionIsExpired,
} from "../../src/lib/server/auth/session";

describe("session helpers", () => {
  it("invalidates a session after the idle timeout", () => {
    const now = Date.UTC(2026, 3, 18, 18, 0, 0);
    const value = createSessionCookieValue(
      {
        sessionId: "session_123",
        userId: "user_123",
        email: "founder@example.com",
        organizationId: "org_123",
        issuedAt: now - 10 * 60 * 1000,
        authAt: now - 10 * 60 * 1000,
        lastSeenAt: now - 61 * 60 * 1000,
        absoluteExpiresAt: now + 60 * 60 * 1000,
      },
      "test-cookie-password"
    );

    const parsed = parseSessionCookieValue(value, "test-cookie-password", now);

    expect(parsed).toBeNull();
    expect(sessionIsExpired({ lastSeenAt: now - 61 * 60 * 1000, absoluteExpiresAt: now + 60 * 60 * 1000 }, now)).toBe(true);
    expect(sessionIsExpired({ lastSeenAt: now - 60 * 60 * 1000, absoluteExpiresAt: now + 60 * 60 * 1000 }, now)).toBe(true);
    expect(SESSION_COOKIE_NAME).toBe("heiwa_session");
  });

  it("rejects auth older than the sensitive-route threshold", () => {
    const now = Date.UTC(2026, 3, 18, 18, 0, 0);

    expect(() =>
      assertRecentAuth(
        {
          authAt: now - 16 * 60 * 1000,
        },
        now
      )
    ).toThrowError(/reauth/i);
    expect(() =>
      assertRecentAuth(
        {
          authAt: now - 15 * 60 * 1000,
        },
        now
      )
    ).toThrowError(/reauth/i);
  });

  it("rejects bad origins on mutations", () => {
    const request = new Request("https://app.heiwa.ltd/api/internal/provider-credentials", {
      method: "POST",
      headers: {
        origin: "https://evil.example",
      },
    });

    expect(() => assertMutableRequestOrigin(request, "https://app.heiwa.ltd")).toThrowError(/origin/i);
  });

  it("uses insecure cookies for localhost http and secure cookies for https", () => {
    expect(shouldUseSecureCookies(new URL("http://localhost:5173/app"))).toBe(false);
    expect(shouldUseSecureCookies(new URL("https://app.heiwa.ltd/app"))).toBe(true);
  });

  it("accepts matching double-submit csrf tokens and rejects mismatches", () => {
    const token = issueCsrfToken();
    const request = new Request("https://app.heiwa.ltd/api/internal/provider-credentials", {
      method: "POST",
      headers: {
        "x-csrf-token": token,
      },
    });

    expect(buildCsrfCookieHeader(token, new URL("http://localhost:5173/app"))).toContain("heiwa_csrf=");
    expect(buildCsrfCookieHeader(token, new URL("http://localhost:5173/app"))).not.toContain("Secure");
    expect(buildCsrfCookieHeader(token, new URL("https://app.heiwa.ltd/app"))).toContain("Secure");
    expect(() => assertCsrfToken(request, token)).not.toThrow();
    expect(() => assertCsrfToken(request, "different-token")).toThrowError(/csrf/i);
  });
});
