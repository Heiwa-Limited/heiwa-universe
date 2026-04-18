import { redirect } from "@sveltejs/kit";
import type { LayoutServerLoad } from "./$types";

export const load: LayoutServerLoad = async ({ locals, url }) => {
  const authFailed = url.searchParams.get("auth") === "failed";

  if (!locals.auth && !authFailed) {
    throw redirect(303, "/auth/sign-in");
  }

  return {
    auth: locals.auth,
    authFailed: !locals.auth && authFailed,
  };
};
