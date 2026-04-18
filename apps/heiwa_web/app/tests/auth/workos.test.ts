import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  buildLogoutUrl,
  buildSignInUrl,
  getAppOrigin,
  getRedirectUri,
} from "../../src/lib/server/auth/workos";

const workosMocks = vi.hoisted(() => ({
  getAuthorizationUrl: vi.fn(),
  getLogoutUrl: vi.fn(),
  authenticateWithCode: vi.fn(),
}));

vi.mock("@workos-inc/node", () => ({
  WorkOS: vi.fn().mockImplementation(() => ({
    userManagement: {
      getAuthorizationUrl: workosMocks.getAuthorizationUrl,
      getLogoutUrl: workosMocks.getLogoutUrl,
      authenticateWithCode: workosMocks.authenticateWithCode,
    },
  })),
}));

describe("workos origin helpers", () => {
  beforeEach(() => {
    vi.unstubAllEnvs();
    workosMocks.getAuthorizationUrl.mockReset();
    workosMocks.getLogoutUrl.mockReset();
    workosMocks.authenticateWithCode.mockReset();
    workosMocks.getAuthorizationUrl.mockReturnValue("https://auth.workos.local/sign-in");
    workosMocks.getLogoutUrl.mockReturnValue("https://auth.workos.local/logout");
    vi.stubEnv("WORKOS_API_KEY", "test-api-key");
    vi.stubEnv("WORKOS_CLIENT_ID", "test-client-id");
  });

  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it("uses localhost defaults outside production", () => {
    vi.stubEnv("NODE_ENV", "development");

    expect(getAppOrigin()).toBe("http://localhost:5173");
    expect(getRedirectUri()).toBe("http://localhost:5173/auth/callback");
    expect(buildSignInUrl("browser-state")).toBe("https://auth.workos.local/sign-in");
    expect(buildLogoutUrl({ sessionId: "session_123" })).toBe("https://auth.workos.local/logout");
    expect(workosMocks.getAuthorizationUrl).toHaveBeenCalledWith(
      expect.objectContaining({
        clientId: "test-client-id",
        redirectUri: "http://localhost:5173/auth/callback",
        state: "browser-state",
      })
    );
    expect(workosMocks.getLogoutUrl).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: "session_123",
        returnTo: "http://localhost:5173",
      })
    );
  });

  it("requires explicit origins in production", () => {
    vi.stubEnv("NODE_ENV", "production");

    expect(() => getAppOrigin()).toThrow("APP_ORIGIN is required in production");
    expect(() => getRedirectUri()).toThrow("WORKOS_REDIRECT_URI is required in production");
    expect(() => buildSignInUrl("browser-state")).toThrow("WORKOS_REDIRECT_URI is required in production");
    expect(() => buildLogoutUrl({ sessionId: "session_123" })).toThrow("APP_ORIGIN is required in production");
  });

  it("uses explicit production env vars when present", () => {
    vi.stubEnv("NODE_ENV", "production");
    vi.stubEnv("APP_ORIGIN", "https://app.heiwa.ltd");
    vi.stubEnv("WORKOS_REDIRECT_URI", "https://app.heiwa.ltd/auth/callback");

    expect(getAppOrigin()).toBe("https://app.heiwa.ltd");
    expect(getRedirectUri()).toBe("https://app.heiwa.ltd/auth/callback");
    expect(buildSignInUrl("browser-state")).toBe("https://auth.workos.local/sign-in");
    expect(buildLogoutUrl({ sessionId: "session_123" })).toBe("https://auth.workos.local/logout");
    expect(workosMocks.getAuthorizationUrl).toHaveBeenCalledWith(
      expect.objectContaining({
        redirectUri: "https://app.heiwa.ltd/auth/callback",
      })
    );
    expect(workosMocks.getLogoutUrl).toHaveBeenCalledWith(
      expect.objectContaining({
        returnTo: "https://app.heiwa.ltd",
      })
    );
  });
});
