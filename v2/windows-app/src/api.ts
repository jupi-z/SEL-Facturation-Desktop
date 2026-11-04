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
