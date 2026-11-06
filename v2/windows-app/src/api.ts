import { invoke } from "@tauri-apps/api/core";

export type HttpMethod = "GET" | "POST" | "PATCH";
export type CentralResponse<T> = { status: number; body: T; next_cursor?: string | null };

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

function centralHttpError(status: number, body: unknown, _path: string, authority: string): Error {
  const problem = body && typeof body === "object"
    ? body as { message?: unknown; error?: { message?: unknown }; detail?: unknown; code?: unknown; title?: unknown }
    : {};
  const code = typeof problem.code === "string"
    ? problem.code
    : typeof problem.title === "string" ? problem.title : "";
  const businessMessages: Record<string, string> = {
    LAST_ADMINISTRATOR: "Impossible de désactiver ou rétrograder le dernier administrateur actif.",
    SELF_MODIFICATION_DENIED: "Un administrateur ne peut pas modifier son propre rôle ou statut.",
    USER_IDENTITY_EXISTS: "Cet identifiant SEL est déjà utilisé ou invalide.",
    CUSTOMER_REVISION_CONFLICT: "Cette fiche a été modifiée entre-temps. Rechargez-la et vérifiez les changements avant de réessayer.",
    CUSTOMER_CODE_EXISTS: "Ce code abonné est déjà attribué.",
    INVOICE_PERIOD_EXISTS: "Une facture existe déjà pour cet abonné et cette période.",
    METER_READING_NOT_BILLABLE: "Les relevés ont changé ou sont déjà facturés. Actualisez les relevés avant de prévisualiser.",
    RECEIVABLE_OVERALLOCATED: "Le paiement dépasserait le solde restant de la créance.",
    PAYMENT_NOT_REVERSIBLE: "Ce paiement n’est pas dans un état permettant une contrepassation.",
    PAYMENT_REVERSAL_OVERALLOCATED: "Le montant dépasse la partie du paiement encore réversible.",
    TARIFF_PERIOD_OVERLAP: "Un tarif approuvé couvre déjà une partie de cette période. Vérifiez les dates d’effet.",
    SOLAR_PRODUCT_PERIOD_OVERLAP: "Une version approuvée de ce produit couvre déjà une partie de cette période.",
    TARIFF_NOT_DRAFT: "Ce tarif n’est plus un brouillon. Actualisez le catalogue avant de continuer.",
    SOLAR_PRODUCT_NOT_DRAFT: "Ce produit n’est plus un brouillon. Actualisez le catalogue avant de continuer.",
    IDEMPOTENCY_PAYLOAD_MISMATCH: "La demande reprend une référence déjà utilisée avec des données différentes. Actualisez avant de réessayer.",
    PAYMENT_CONTEXT_INVALID: "Le client ou la devise du paiement ne correspond pas à la créance.",
    PAYMENT_ALLOCATION_EXCEEDS_RECEIPT: "Le total affecté dépasse le montant reçu.",
  };
  if (code && businessMessages[code]) return new Error(businessMessages[code]);

  if (status === 403) {
    const reason = code === "PERMISSION_DENIED"
      ? "Le rôle de ce compte n’autorise pas cette opération."
      : code === "PASSWORD_CHANGE_REQUIRED"
        ? "Changez le mot de passe temporaire avant de continuer."
        : "Le serveur SEL a refusé cette opération. Vérifiez le rôle du compte ou demandez à un administrateur.";
    return new Error(reason);
  }

  if (status === 401) return new Error("La session SEL a expiré ou les identifiants ne sont pas valides. Reconnectez-vous.");
  if (status === 404) return new Error("L’élément demandé n’existe plus ou n’est pas accessible. Actualisez la liste.");
  if (status === 409) return new Error("Le serveur a détecté un changement ou un conflit. Actualisez les données avant de réessayer.");
  if (status === 422) return new Error("Certaines données ne sont pas acceptées par les règles SEL. Vérifiez les champs et les paramètres approuvés.");
  if (status === 429) return new Error("Trop de demandes ont été envoyées. Patientez un instant puis réessayez.");
  if (status >= 500) return new Error("Le service SEL rencontre un problème temporaire. Aucune confirmation n’a été reçue ; vérifiez l’historique avant de relancer l’opération.");
  return new Error(`${authority} n’a pas accepté la demande (HTTP ${status}).`);
}

export async function centralRequestPage<T>(
  path: string,
  method: HttpMethod = "GET",
  body?: unknown,
  ifMatch?: number,
): Promise<CentralResponse<T>> {
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
  return response;
}

export async function centralRequest<T>(
  path: string,
  method: HttpMethod = "GET",
  body?: unknown,
  ifMatch?: number,
): Promise<T> {
  return (await centralRequestPage<T>(path, method, body, ifMatch)).body;
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
