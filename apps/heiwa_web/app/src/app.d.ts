/// <reference types="@sveltejs/kit" />

import type { SessionRecord } from "./lib/server/auth/session";

declare global {
  namespace App {
    interface Locals {
      auth: SessionRecord | null;
    }
  }
}

export {};
