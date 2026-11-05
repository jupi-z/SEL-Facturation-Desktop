import { invoke } from "@tauri-apps/api/core";

export type HttpMethod = "GET" | "POST" | "PATCH";
export type LocalResponse<T> = { status: number; body: T };

export async function localRequest<T>(
  path: string,
  method: HttpMethod = "GET",
  body?: unknown,
  ifMatch?: number,
): Promise<T> {
  const response = await invoke<LocalResponse<T>>("local_request", { path, method, body: body ?? null, ifMatch: ifMatch ?? null });
  if (response.status < 200 || response.status >= 300) {
    const detail = response.body as { message?: string; error?: { message?: string } };
    throw new Error(detail?.error?.message ?? detail?.message ?? `Le service local a répondu HTTP ${response.status}.`);
  }
  return response.body;
}

export function localStatus<T>(): Promise<T> {
  return invoke<T>("local_status");
}

export function openMonarqueSite(): Promise<void> {
  return invoke<void>("open_monarque_site");
}

export type CentralSessionInfo = {
  authenticated: boolean;
  user_id?: string;
  organization_id?: string;
  role?: string;
  require_password_change?: boolean;
};

export function centralLogin(identityRef: string, password: string): Promise<Omit<CentralSessionInfo, "authenticated">> {
  return invoke("central_login", { identityRef, password });
}

export function centralSessionStatus(): Promise<CentralSessionInfo> {
  return invoke("central_session_status");
}

export async function centralAdminRequest<T>(path: string, method: HttpMethod = "GET", body?: unknown): Promise<T> {
  const response = await invoke<LocalResponse<T>>("central_admin_request", { path, method, body: body ?? null });
  if (response.status < 200 || response.status >= 300) {
    const detail = response.body as { message?: string; error?: { message?: string } };
    throw new Error(detail?.error?.message ?? detail?.message ?? `Le serveur central a répondu HTTP ${response.status}.`);
  }
  return response.body;
}

export function centralLogout(): Promise<void> {
  return invoke<void>("central_logout");
}
