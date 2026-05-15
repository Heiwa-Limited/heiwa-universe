import providersData from "../../../web/assets/providers.json";

export type Lane = "oauth_cli" | "subscription" | "api_key" | "local";
export type Maturity = "stable" | "beta" | "experimental" | "planned";

export interface Provider {
  id: string;
  name: string;
  vendor: string;
  lanes: Lane[];
  subscriptions?: string[];
  maturity: Maturity;
  rate_group?: string;
  priority?: number;
  description?: string;
  models_example?: string[];
  homepage?: string;
}

export interface ProvidersDoc {
  version: string;
  updated: string;
  lanes: Record<
    Lane,
    { label: string; short: string; description: string; lead?: boolean }
  >;
  maturity: Record<Maturity, { label: string; description: string }>;
  providers: Provider[];
  rate_groups: Record<string, { priority: number; description: string }>;
}

export const providers = providersData as unknown as ProvidersDoc;
