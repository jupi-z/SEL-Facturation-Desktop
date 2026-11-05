import { invoke } from "@tauri-apps/api/core";

export type HttpMethod = "GET" | "POST" | "PATCH";
export type CentralResponse<T> = { status: number; body: T };

async function writeIdempotencyKey(method: HttpMethod, path: string, body: unknown): Promise<string | null> {
  if (method === "GET") return null;
  const fingerprint = `${method}\n${path}\n${JSON.stringify(body ?? null)}`;
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(fingerprint));
  const fingerprintKey = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
  const storageKey = `sel-central-idempotency-v1:${fingerprintKey}`;
  const existing = localStorage.getItem(storageKey);
  if (existing) {
    try {
      const saved = JSON.parse(existing) as { key?: string; created_at?: number };
      if (saved.key && Date.now() - Number(saved.created_at) < 72 * 60 * 60 * 1000) return saved.key;
    } catch { /* Replace an invalid local retry marker. */ }
  }
  const key = crypto.randomUUID();
  localStorage.setItem(storageKey, JSON.stringify({ key, created_at: Date.now() }));
  return key;
}

async function clearIdempotencyKey(method: HttpMethod, path: string, body: unknown): Promise<void> {
  if (method === "GET") return;
  const fingerprint = `${method}\n${path}\n${JSON.stringify(body ?? null)}`;
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(fingerprint));
  const fingerprintKey = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
  localStorage.removeItem(`sel-central-idempotency-v1:${fingerprintKey}`);
}

function reportCentralHttpFailure(status: number): void {
  if (status === 401) {
    window.dispatchEvent(new Event("sel-session-expired"));
  } else if (status === 502 || status === 503 || status === 504) {
    window.dispatchEvent(new Event("sel-server-unavailable"));
  }
}

function centralHttpError(status: number, body: unknown, path: string, authority: string): Error {
  const problem = body && typeof body === "object"
    ? body as { message?: unknown; error?: { message?: unknown }; detail?: unknown; code?: unknown; title?: unknown }
    : {};
  const code = typeof problem.code === "string"
    ? problem.code
    : typeof problem.title === "string" ? problem.title : "";
  const detail = typeof problem.detail === "string"
    ? problem.detail
    : typeof problem.error?.message === "string" ? problem.error.message
      : typeof problem.message === "string" ? problem.message : "";

  if (status === 403) {
    const reason = code === "PERMISSION_DENIED"
      ? "Le rôle de ce compte n’autorise pas cette opération."
      : code === "PASSWORD_CHANGE_REQUIRED"
        ? "Changez le mot de passe temporaire avant de continuer."
        : detail || "Le serveur a refusé l’accès à cette ressource.";
    return new Error(`${authority} — accès refusé (HTTP 403${code ? ` · ${code}` : ""}) sur ${path}. ${reason}`);
  }

  return new Error(detail || `${authority} a répondu HTTP ${status} pour ${path}.`);
}

export async function centralRequest<T>(
  path: string,
  method: HttpMethod = "GET",
  body?: unknown,
  ifMatch?: number,
): Promise<T> {
  let response: CentralResponse<T>;
  const idempotencyKey = await writeIdempotencyKey(method, path, body);
  try {
    response = await invoke<CentralResponse<T>>("central_request", { path, method, body: body ?? null, ifMatch: ifMatch ?? null, idempotencyKey });
  } catch (reason) {
    const health = await centralStatus<{ configured?: boolean }>().catch(() => null);
    if (!health) window.dispatchEvent(new Event("sel-server-unavailable"));
    else if (health.configured === true) {
      const status = await centralSessionStatus().catch(() => ({ authenticated: false }));
      if (!status.authenticated) window.dispatchEvent(new Event("sel-session-expired"));
    }
    throw reason;
  }
  if (response.status < 200 || response.status >= 300) {
    reportCentralHttpFailure(response.status);
    throw centralHttpError(response.status, response.body, path, "Le serveur SEL");
  }
  await clearIdempotencyKey(method, path, body);
  return response.body;
}

export function centralStatus<T>(): Promise<T> {
  return invoke<T>("central_status");
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
  server_reachable?: boolean;
};

export function centralLogin(identityRef: string, password: string): Promise<Omit<CentralSessionInfo, "authenticated">> {
  return invoke("central_login", { identityRef, password });
}

export function centralSessionStatus(): Promise<CentralSessionInfo> {
  return invoke("central_session_status");
}

export async function centralAdminRequest<T>(path: string, method: HttpMethod = "GET", body?: unknown): Promise<T> {
  let response: CentralResponse<T>;
  const idempotencyKey = await writeIdempotencyKey(method, path, body);
  try {
    response = await invoke<CentralResponse<T>>("central_admin_request", { path, method, body: body ?? null, ifMatch: null, idempotencyKey });
  } catch (reason) {
    const health = await centralStatus<{ configured?: boolean }>().catch(() => null);
    if (!health) window.dispatchEvent(new Event("sel-server-unavailable"));
    else if (health.configured === true) {
      const status = await centralSessionStatus().catch(() => ({ authenticated: false }));
      if (!status.authenticated) window.dispatchEvent(new Event("sel-session-expired"));
    }
    throw reason;
  }
  if (response.status < 200 || response.status >= 300) {
    reportCentralHttpFailure(response.status);
    throw centralHttpError(response.status, response.body, path, "Le serveur central");
  }
  await clearIdempotencyKey(method, path, body);
  return response.body;
}

export function centralLogout(): Promise<void> {
  return invoke<void>("central_logout");
}
