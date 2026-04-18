import { beforeEach, describe, expect, it, vi } from "vitest";
import { GET as signInGet } from "../../src/routes/auth/sign-in/+server";

const authMocks = vi.hoisted(() => ({
  issueAuthStateToken: vi.fn(),
  buildSignInUrl: vi.fn(),
}));

vi.mock("../../src/lib/server/auth/workos", () => ({
  AUTH_STATE_COOKIE_NAME: "heiwa_auth_state",
  AUTH_STATE_MAX_AGE_MS: 15 * 60 * 1000,
  buildSignInUrl: authMocks.buildSignInUrl,
  issueAuthStateToken: authMocks.issueAuthStateToken,
}));

describe("auth sign-in route", () => {
  beforeEach(() => {
    authMocks.issueAuthStateToken.mockReset();
    authMocks.buildSignInUrl.mockReset();
    authMocks.issueAuthStateToken.mockReturnValue("browser-state");
    authMocks.buildSignInUrl.mockReturnValue("https://auth.heiwa.ltd/sign-in?state=browser-state");
  });

  it("issues a browser-bound state cookie before redirecting to WorkOS", async () => {
    const set = vi.fn();
    const request = new Request("http://localhost:5173/auth/sign-in", {
      method: "GET",
    });

    const response = await signInGet({
      url: new URL(request.url),
      request,
      cookies: {
        set,
        get: vi.fn(),
        delete: vi.fn(),
      },
    } as never);

    expect(authMocks.issueAuthStateToken).toHaveBeenCalledTimes(1);
    expect(authMocks.buildSignInUrl).toHaveBeenCalledWith("browser-state");
    expect(set).toHaveBeenCalledWith(
      "heiwa_auth_state",
      "browser-state",
      expect.objectContaining({
        httpOnly: true,
        path: "/auth",
        sameSite: "lax",
        secure: false,
        maxAge: 15 * 60,
      })
    );
    expect(response.status).toBe(303);
    expect(response.headers.get("location")).toBe("https://auth.heiwa.ltd/sign-in?state=browser-state");
  });
});
