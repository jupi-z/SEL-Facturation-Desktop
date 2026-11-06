from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "v2" / "windows-app"


def test_v2_windows_client_uses_the_approved_tauri_react_stack() -> None:
    package = json.loads((APP / "package.json").read_text(encoding="utf-8"))
    manifest = (APP / "src-tauri" / "Cargo.toml").read_text(encoding="utf-8")
    config = json.loads((APP / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))

    assert "react" in package["dependencies"]
    assert "@tauri-apps/api" in package["dependencies"]
    assert "build" in package["scripts"]
    assert 'name = "sel-v2-windows"' in manifest
    assert config["productName"] == "SEL Facturation"
    assert config["app"]["windows"][0]["width"] == 1440


def test_tauri_installer_is_per_user_and_windows_ci_exercises_profile_preservation() -> None:
    config = json.loads((APP / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))
    workflow = (ROOT / ".github" / "workflows" / "sel-v2-tauri-windows.yml").read_text(encoding="utf-8")
    smoke = (APP / "scripts" / "installer_lifecycle_smoke.py").read_text(encoding="utf-8")

    assert config["bundle"]["windows"]["nsis"]["installMode"] == "currentUser"
    assert '"version": "0.3.99"' in workflow
    assert "installer_lifecycle_smoke.py" in workflow
    assert 'Path(os.environ["LOCALAPPDATA"]) / APP_IDENTIFIER' in smoke
    assert "central-session.dpapi" in smoke
    assert "local business database" in smoke
    assert "sel-local-v2.sqlite" not in smoke
    assert '"repair/reinstall"' in smoke


def test_v2_windows_client_uses_only_the_authenticated_central_api() -> None:
    adapter = (APP / "src" / "api.ts").read_text(encoding="utf-8")
    native = (APP / "src-tauri" / "src" / "lib.rs").read_text(encoding="utf-8")

    assert 'invoke<CentralResponse<T>>("central_request"' in adapter
    assert "Authorization" not in adapter
    assert 'DEFAULT_CENTRAL_API_URL: &str = "https://sel.shuleops.com"' in native
    assert ".bearer_auth(admin_request.token)" in native
    assert "session.access_token" in native
    assert "local_request" not in native
    assert '"/local/v1/customers"' in native
    assert '"https://' not in adapter


def test_v2_windows_client_exposes_the_figma_primary_workflows() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    for label in (
        "Vue d’ensemble",
        "Abonnés",
        "Facturation",
        "Factures",
        "Journal de caisse",
        "Paiements",
        "Catalogue des kits solaires",
        "Créances",
        "État du serveur",
        "Administration",
        "Prévisualiser la facture",
    ):
        assert label in app

    assert 'label: "Solaire"' in app
    assert 'id: "readings", label: "Relevés"' in app

    assert 'name="tax_id"' not in app
    assert 'customerCreateOpen && <Panel title="Créer un abonné"' in app
    assert '"+ Ajouter un abonné"' in app


def test_ux3_confirmations_are_in_app_accessible_and_visually_exercised() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert "window.confirm" not in app
    assert 'role="alertdialog" aria-modal="true"' in app
    assert 'event.key === "Escape"' in app
    assert "confirmationPreviousFocus" in app
    assert 'f"customer-archive-confirmation-{SHOT_SUFFIX}.png"' in visual_review
    assert 'document.activeElement?.textContent?.trim()' in visual_review
    assert 'Shift+Tab from the first dialog action must wrap to the last action' in visual_review
    assert 'Tab from the last dialog action must wrap to the first action' in visual_review
    assert 'Escape must cancel the archive without sending a write' in visual_review


def test_meter_reading_screen_uses_central_routes_and_displays_rust_measurements() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert '/service-accounts/${encodeURIComponent(accountId)}/meters' in app
    assert '/readings`, "POST"' in app
    assert 'measured_quantity' in app
    assert 'source: "MANUAL"' in app
    assert 'Number(form.get("previous"))' not in app
    assert '/meter-readings/${encodeURIComponent(text(correctingReading.id))}/corrections' in app
    assert "Le relevé initial est conservé." in app
    assert '("readings", "Facturation", "Relevés")' in visual_review
    assert '"DEMO-KWH-001"' in visual_review
    assert 'title={isWaterMeter ? "Relevé d’eau au m³" : "Relevés de compteurs"}' in app
    assert '/v1/meters/${encodeURIComponent(meterId)}/replace' in app
    assert "onConfirmReplacement" in app
    assert 'f"meter-replacement-confirmation-{SHOT_SUFFIX}.png"' in visual_review


def test_v2_windows_client_has_local_brand_assets_and_no_remote_runtime_assets() -> None:
    html = (APP / "index.html").read_text(encoding="utf-8")
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")

    assert (APP / "public" / "assets" / "sel-logo-dark.svg").is_file()
    assert (APP / "public" / "assets" / "geist-variable.woff2").is_file()
    assert (APP / "src" / "vite-env.d.ts").is_file()
    assert "https://" not in html
    assert "https://" not in css
    assert "prefers-reduced-motion" in css


def test_v2_windows_shell_preserves_figma_desktop_geometry_and_tokens() -> None:
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")

    assert "--navy: #012a5e" in css
    assert "--blue: #075bbb" in css
    assert "--yellow: #fddc02" in css
    assert ".sidebar { position: sticky" in css
    assert "flex: 0 0 250px" in css
    assert ".topbar { display: flex; flex: 0 0 76px" in css
    assert "height: 101.409px" in css
    assert "width: 20px; height: 20px; color: #98b9e0" in css


def test_sidebar_uses_distinct_icons_for_each_destination() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    icon = app.split("function NavIcon(", 1)[1].split("\n}\n", 1)[0]

    for name in ("grid", "people", "meter", "document", "cash", "sun", "balance", "server", "settings"):
        assert f"    {name}:" in icon
    assert "void name" not in icon
    assert "stroke=\"currentColor\"" in icon


def test_dashboard_prioritizes_central_workflows_without_demo_financial_data() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert 'title="Que souhaitez-vous faire ?"' in app
    assert 'title="Vos repères"' in app
    assert 'title="Dossiers / activité récente"' not in app
    assert "Émettre une facture" in app
    assert "Encaisser un règlement" in app
    assert "Saisir un relevé" in app
    assert "Vendre un kit solaire" in app
    assert "value={customers.length}" in app
    assert "value={invoices.length}" in app
    assert 'onClick={() => void openInvoiceCreation()}>Nouvelle facture' in app
    assert "218,40" not in app
    assert "1 124,20" not in app


def test_financial_amounts_are_formatted_without_client_side_calculation() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    formatter = app.split("function formatMoney(", 1)[1].split("\n}\n", 1)[0]

    assert "padEnd(2, \"0\")" in formatter
    assert "replace(/\\B(?=(\\d{3})+(?!\\d))/g" in formatter
    assert "Number(" not in formatter
    assert "formatMoney(data.total, data.currency)" in app
    assert "formatMoney(row.balance, row.currency)" in app


def test_cash_journal_uses_a_wide_table_and_lubumbashi_datetime_format() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'className="payment-workspace"' in app
    assert 'title="Encaissements enregistrés"' in app
    assert 'className="payment-table"' in app
    assert 'timeZone: "Africa/Lubumbashi"' in app
    assert 'formatDateTime(row.paid_at ?? row.occurred_at)' in app
    assert ".payment-table { table-layout: fixed; }" in css
    assert 'viewport={"width": VIEWPORT_WIDTH, "height": VIEWPORT_HEIGHT}' in visual_review
    assert 'locale="fr-CD"' in visual_review


def test_solar_sales_use_the_approved_central_catalog_and_rust_quote() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    local_api = (ROOT / "v2" / "crates" / "sel-local" / "src" / "lib.rs").read_text(encoding="utf-8")
    storage = (ROOT / "v2" / "crates" / "sel-storage" / "src" / "sqlite.rs").read_text(encoding="utf-8")

    assert '"/local/v1/solar/products"' in local_api
    assert "list_active_solar_products" in storage
    assert '"/v1/catalog/products"' in app
    assert '"/v1/solar/quotes"' in app
    assert '"/v1/solar/contracts"' in app
    assert 'v1/installment-plans/${encodeURIComponent(planId)}' in app
    assert 'centralRequest<unknown>("/v1/catalog/products")' in app
    assert "Vérifier le devis" in app
    assert "Créer le contrat" in app
    assert "Échéancier consulté" in app
    assert "overdue_amount" in app
    assert "principal: result.principal" in app
    assert 'name="principal"' not in app
    assert "innerHTML" not in app
    assert "<NavIcon" in app


def test_invoice_workspace_matches_figma_two_column_preview_flow() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'className="invoice-workspace"' in app
    assert 'className="panel invoice-summary"' in app
    assert 'form="utility-invoice-form"' in app
    assert "formatMoney(preview.total, preview.currency)" in app
    assert ".invoice-workspace { display: grid; grid-template-columns:" in css
    assert 'f"invoice-preview-{SHOT_SUFFIX}.png"' in visual_review
    assert "path === '/v1/billing/utility/preview'" in visual_review


def test_invoice_preview_is_a_distinct_figma_aligned_step_before_emission() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")
    invoice_view = app.split('{view === "invoices"', 1)[1].split('{view === "payments"', 1)[0]

    assert '{!preview && invoiceWorkspace === "history"' in invoice_view
    assert 'invoiceWorkspace === "create" && <div hidden={Boolean(preview)}>' in invoice_view
    assert 'setInvoiceWorkspace("history")' in invoice_view
    assert 'title="Prévisualisation de facture"' in invoice_view
    assert 'className="invoice-preview-metrics metrics"' in invoice_view
    assert 'label="Électricité"' in invoice_view
    assert 'label="Eau"' in invoice_view
    assert 'setInvoiceWorkspace("create")' in invoice_view
    assert 'onClick={() => void confirmInvoice()}>Émettre la facture' in invoice_view
    assert 'get_by_role("heading", name="Prévisualisation de facture")' in visual_review
    assert 'get_by_role("button", name="Modifier la saisie")' in visual_review


def test_solar_catalog_is_the_primary_page_and_sale_entry_is_actionable() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'title={solarWorkspace === "sales" ? "Catalogue des kits solaires" : "Contrats et échéanciers"}' in app
    assert 'aria-label="Pages solaire"' in app
    assert 'navigateSolarWorkspace("contracts")' in app
    assert 'navigateSolarWorkspace("sales")' in app
    assert 'title: "Abandonner la saisie solaire ?"' in app
    assert "Créer une vente" in app
    assert 'className="metrics solar-metrics"' in app
    assert 'headers={["SKU", "Produit", "Catégorie", "Prix de référence", "État"]}' in app
    assert "solarSaleOpen &&" in app
    assert ".solar-metrics { grid-template-columns: repeat(4, minmax(0, 1fr));" in css
    assert 'f"{slug}-{SHOT_SUFFIX}.png"' in visual_review
    assert 'f"solar-new-sale-{SHOT_SUFFIX}.png"' in visual_review
    assert 'f"solar-contracts-{SHOT_SUFFIX}.png"' in visual_review
    assert 'solar_draft_review' in visual_review


def test_navigation_clears_transient_notices_from_the_previous_screen() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert "function navigateTo(nextView: View, customerId?: string)" in app
    navigation = app.split("function navigateTo(nextView: View, customerId?: string)", 1)[1].split("\n  function navigateBack", 1)[0]
    assert "setNotice(null)" in navigation
    assert "setView(nextView)" in navigation
    assert "onClick={() => navigateTo(item.target)}" in app
    assert 'onClick={() => void openInvoiceCreation()}>Nouvelle facture' in app


def test_invoice_history_matches_figma_f07_and_keeps_creation_separate() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'title="Historique des factures"' in app
    assert 'className="metrics invoice-history-metrics"' in app
    assert 'className="invoice-history-layout"' in app
    assert 'title="Actions suivantes"' in app
    assert 'Nouvelle facture' in app and 'Émettre une facture combinée' in app
    assert 'Factures enregistrées' in app
    assert '.invoice-history-layout { display: grid;' in css
    assert 'invoice-history-{SHOT_SUFFIX}.png' in visual_review
    assert 'four server-derived metrics from Figma F07' in visual_review


def test_solar_sales_show_customer_names_instead_of_internal_ids() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert "function customerLabel(customerId: unknown, customers: Row[])" in app
    assert "customerLabel(row.customer_id, customers)" in app
    assert "<SolarContractManager sales={sales} customers={customers}" in app
    assert "onRecordPayment={async (customerId, receivableId, scheduleId)" in app
    assert 'onClick={() => onRecordPayment(text(plan.customer_id), text(plan.receivable_id), text(nextOpenSchedule.id))}' in app
    assert "initialReceivableId={paymentPrefill?.receivableId ?? \"\"}" in app
    assert "initialScheduleId={paymentPrefill?.scheduleId ?? \"\"}" in app
    assert 'get_by_role("button", name="Enregistrer un versement")' in (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")


def test_solar_catalog_route_uses_local_api_error_contract() -> None:
    local_api = (ROOT / "v2" / "crates" / "sel-local" / "src" / "lib.rs").read_text(encoding="utf-8")
    handler = local_api.split("async fn list_solar_products(", 1)[1].split("\n}\n", 1)[0]

    assert "Result<Json<Vec<serde_json::Value>>, (StatusCode, String)>" in handler
    assert ".map_err(internal_error)?" in handler


def test_admin_console_uses_individual_session_without_offline_device_controls() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    adapter = (APP / "src" / "api.ts").read_text(encoding="utf-8")
    native = (APP / "src-tauri" / "src" / "lib.rs").read_text(encoding="utf-8")

    assert "centralLogin" in app
    assert '"/v1/admin/users"' in app
    assert '"/v1/admin/devices"' not in app
    assert '"/v1/admin/offline-grants"' not in app
    assert '"/v1/admin/audit-events"' in app
    assert "API locale d’administration non exposée" not in app
    assert "Vérifier la création" in app
    assert "Désactiver" in app and "Réactiver" in app
    assert "Réinitialiser le mot de passe" in app
    assert '`/v1/devices/${encodeURIComponent(id)}/revoke`' not in app
    assert '`/v1/admin/offline-grants/${encodeURIComponent(id)}/revoke`' not in app
    assert "minLength={8}" in app
    assert "AdminCatalogPanel" in app
    assert 'className="admin-workspace-tabs"' in app
    assert 'className="admin-directory-layout"' in app
    assert 'className="customer-details admin-user-summary"' in app
    assert 'aria-label={`Voir les accès de ${text(user.identity_ref)}`}' in app
    assert 'Modifier les accès d’un compte' in app
    assert 'Nouveau rôle de ${text(selectedAdminUser.identity_ref)}' in app
    assert 'navigateAdminArea("users", true)' in app
    assert ">Référentiels</Button>" in app and ">Journal d’audit</Button>" in app
    assert "Créer un brouillon" in app
    assert "Référence de la décision d’approbation SEL" in app
    assert "Journal d’audit récent" in app
    assert '"/local/v1/' not in app
    assert "Synchronisation" not in app
    assert 'invoke("central_login"' in adapter
    assert '"central_admin_request"' in adapter
    assert "central_login" in native and "central_admin_request" in native
    assert "central_session" in native
    assert "/v1/auth/login" in native
    assert "Authorization" not in adapter


def test_admin_initial_password_checkbox_keeps_intrinsic_control_size() -> None:
    styles = (APP / "src" / "App.css").read_text(encoding="utf-8")
    assert '.admin-create-user input[type="checkbox"]' in styles
    assert "width: 17px; min-width: 17px; min-height: 17px; height: 17px" in styles
    assert ".admin-create-user label > span:last-child { display: flex; align-items: center" in styles


def test_admin_user_mutations_require_a_review_and_keep_password_out_of_dialog() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert "confirmAction({" in app
    assert 'title: "Vérifier la création du compte"' in app
    assert 'title: disabling ? "Désactiver ce compte ?" : "Réactiver ce compte ?"' in app
    assert 'title: "Vérifier le changement de rôle"' in app
    assert 'title: "Vérifier la réinitialisation du mot de passe"' in app
    assert 'title: "Vérifier la réinitialisation du mot de passe"' in app
    assert "Le mot de passe reste masqué." in app
    assert 'name="password_confirm"' in app
    assert "Les deux mots de passe ne correspondent pas." in app
    assert "userMutationBusy" in app
    assert 'title: "Vérifier l’approbation du référentiel"' in app
    assert 'title: "Vérifier la révocation du référentiel"' in app
    assert "window.__adminWriteLog" in visual_review
    assert "Canceling user creation must not send a write request" in visual_review
    assert "window.__adminWriteLog.filter(entry => entry.path === '/v1/admin/users' && entry.method === 'POST').length === 1" in visual_review
    assert "Role updates must wait for explicit confirmation" in visual_review
    assert "Mismatched initial passwords must not send a write request" in visual_review
    assert "Catalog approval must wait for explicit confirmation" in visual_review
    assert "Catalog revocation must wait for explicit confirmation" in visual_review
    assert "Catalog drafts must not be submitted before explicit review" in visual_review
    assert "Password reset must wait for explicit confirmation" in visual_review
    assert "Catalog drafts must not be submitted before explicit review" in visual_review
    assert "Password reset must wait for explicit confirmation" in visual_review
    assert 'f"admin-catalog-approval-{SHOT_SUFFIX}.png"' in visual_review
    assert 'f"admin-catalog-revocation-{SHOT_SUFFIX}.png"' in visual_review


def test_native_central_admin_bridge_is_allowlisted_and_never_uses_sync_token_as_user_session() -> None:
    native = (APP / "src-tauri" / "src" / "lib.rs").read_text(encoding="utf-8")

    assert "validate_central_admin_request" in native
    assert "session.access_token" in native
    admin_command = native.split("async fn central_admin_request(", 1)[1].split("\n#[tauri::command]", 1)[0]
    assert "central_api_token" not in admin_command
    assert "v1/auth/login" in native
    assert '"tariffs" | "products" | "tax-profiles" | "fx-rates"' in native
    assert '"", "v1", "devices", id, "revoke"' not in native
    assert '"/v1/users/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/password"' in native
    assert "localStorage" not in (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    assert ".." in native


def test_admin_reference_writes_use_native_idempotency_and_require_explicit_approval() -> None:
    native = (APP / "src-tauri" / "src" / "lib.rs").read_text(encoding="utf-8")
    manifest = (APP / "src-tauri" / "Cargo.toml").read_text(encoding="utf-8")
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert 'tokio = { version = "1", features = ["sync"] }' in manifest
    assert 'header("Idempotency-Key", admin_request.idempotency_key)' in native
    assert "effective_to: values.effective_to || null" in app
    assert 'approval_ref: approvalRef' in app
    assert 'supersedes_tariff_id' in app
    assert 'supersedes_product_id' in app
    assert 'onRevoke={revokeCatalogItem}' in app
    assert 'change_ref: changeRef' in app
    assert "Brouillon" in app


def test_phase_seven_audit_and_sync_issue_contracts_are_documented() -> None:
    openapi = (ROOT / "v2" / "openapi" / "sel-v2.yaml").read_text(encoding="utf-8")
    local_contract = (ROOT / "Docs" / "SEL_V2_BACKEND" / "03_CONTRATS_API_ET_AUTONOMIE.md").read_text(encoding="utf-8")

    assert "/v1/admin/audit-events:" in openapi
    assert "organization through an actor or device" in openapi
    assert "/local/v1/sync/issues" in local_contract
    assert "ne sont jamais résolues automatiquement" in local_contract


def test_visual_review_covers_mandatory_central_login_and_authenticated_state() -> None:
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'f"login-{SHOT_SUFFIX}.png"' in visual_review
    assert 'get_by_role("heading", name="Connexion à SEL Facturation")' in visual_review
    assert 'get_by_role("button", name="Se connecter")' in visual_review
    assert "__ADMIN_DEMO_SESSION__" in visual_review
    assert "demo-admin-0001" in visual_review
    assert "central_admin_request" in visual_review
    assert "demo-audit-0001" in visual_review
    assert "central_request" in visual_review
    assert "/local/v1/" not in visual_review
    assert "PENDING_NETWORK" not in visual_review


def test_solar_payment_form_clears_stale_schedules_and_fails_closed() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    payment_form = app.split("function PaymentForm(", 1)[1].split("\n}\n\nfunction MeterReadingsView", 1)[0]

    assert "if (!schedulePlanKey) { setPlanSchedules({}); return; }" in payment_form
    assert "loading: true, error: \"\"" in payment_form
    assert "error: humanError(error)" in payment_form
    assert "disabled={!planState || planState.loading || Boolean(planState.error)}" in payment_form
    assert "schedule_id: allocation.scheduleId || null" in payment_form


def test_payment_form_clears_previous_customer_receivables_while_loading() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    payment_form = app.split("function PaymentForm(", 1)[1].split("\n}\n\nfunction MeterReadingsView", 1)[0]

    assert "setReceivables([]); setReceivablesLoading(true); setReceivablesError(\"\")" in payment_form
    assert "disabled={!customerId || receivablesLoading || Boolean(receivablesError) || !receivables.length}" in payment_form
    assert "const paymentBlocked = receivablesLoading || Boolean(receivablesError) || !allocationsReady" in payment_form
    assert "disabled={paymentBlocked || Boolean(paymentReview)}" in payment_form


def test_payment_requires_a_separate_review_before_posting_to_server() -> None:
    app = (ROOT / "v2" / "windows-app" / "src" / "App.tsx").read_text(encoding="utf-8")
    payment_form = app.split("function PaymentForm(", 1)[1].split("\n}\n\nfunction MeterReadingsView", 1)[0]

    initial_submit = payment_form.split('onSubmit={(event) => {', 1)[1].split("}}>\n    <label", 1)[0]
    assert "event.preventDefault()" in initial_submit
    assert "setPaymentReview({" in initial_submit
    assert "onSubmit(form, \"/v1/payments\"" not in initial_submit
    assert 'paymentReview ? <section className="payment-review"' in payment_form
    assert 'saving ? "Enregistrement…"' in payment_form
    assert 'onClick={() => void confirmPayment()}' in payment_form
    assert "disabled={saving}" in payment_form
    assert "savingRef.current" in payment_form


def test_payment_receivable_choices_show_server_invoice_or_sale_reference() -> None:
    app = (ROOT / "v2" / "windows-app" / "src" / "App.tsx").read_text(encoding="utf-8")
    payment_form = app.split("function PaymentForm(", 1)[1].split("\n}\n\nfunction MeterReadingsView", 1)[0]
    openapi = (ROOT / "v2" / "openapi" / "sel-v2.yaml").read_text(encoding="utf-8")

    assert "text(item.source_reference, id)" in payment_form
    assert "source_type: { type: string, enum: [INVOICE, SALE] }" in openapi


def test_catalog_draft_form_resets_only_after_server_accepts_creation() -> None:
    app = (ROOT / "v2" / "windows-app" / "src" / "App.tsx").read_text(encoding="utf-8")
    panel = app.split("function AdminCatalogPanel(", 1)[1].split("\nfunction AdminConsole", 1)[0]
    create = panel.split("async function create(", 1)[1].split("\n  return <Panel", 1)[0]

    assert "const accepted = await confirmAction({" in create
    assert "if (accepted && await onCreate(kind, values)) { form.reset(); announceDraftSaved(); }" in create
    assert "onCreate: (kind: AdminCatalog, values: Record<string, string>) => Promise<boolean>" in panel
    assert "title: `Vérifier le brouillon · ${definition.title}`" in create
    assert '<form data-preserve-draft className="form-grid admin-catalog-form"' in panel
    assert "return false" in app.split("async function createCatalogDraft(", 1)[1].split("\n  async function approveCatalogDraft", 1)[0]


def test_payment_form_can_allocate_one_receipt_to_multiple_real_receivables() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    payment_form = app.split("function PaymentForm(", 1)[1].split("\n}\n\nfunction MeterReadingsView", 1)[0]

    assert "allocationRows" in payment_form
    assert "Ajouter une créance" in payment_form
    assert "allocations: allocationRows.map" in payment_form
    assert "installmentPlanByReceivable" in payment_form
    assert "Toutes les créances d’un même règlement doivent utiliser la même devise." in payment_form
    review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")
    assert 'f"payments-solar-{SHOT_SUFFIX}.png"' in review
    assert "page.locator('.allocation-card select').first.locator('option[value=\"demo-receivable-0001\"]')" in review
    assert "page.locator('.allocation-card select').nth(2).select_option" in review


def test_windows_typescript_target_does_not_use_es2022_array_at() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert ".at(-1)" not in app
    assert "readings[readings.length - 1]" in app


def test_central_financial_statuses_are_shown_in_french_without_offline_states() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert 'CENTRAL_ACCEPTED: "Confirmé par le serveur"' in app
    assert 'LOCAL_PENDING:' not in app
    assert 'LOCAL_ACCEPTED:' not in app
    assert 'OVERDUE: "En retard"' in app


def test_customer_statement_uses_localized_status_badges() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    statement = app.split("function CustomerStatement(", 1)[1].split("\ntype AdminCatalog", 1)[0]

    assert statement.count("<Status value={item.status} />") == 2
    assert "text(item.status)" not in statement
    assert "customerName: string" in statement
    assert "Situation du compte · ${customerName}" in statement
    assert '<CustomerStatement customerName={text(selectedCustomerData?.legal_name, selectedCustomer)}' in app


def test_customer_statement_is_scoped_to_the_selected_customer_during_loading() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert "const currentStatement = statementState.customerId === selectedCustomer ? statementState : null;" in app
    assert "setStatementState({ customerId: selectedCustomer, data: null, loading: true, error: \"\" });" in app
    assert "loading={currentStatement?.loading ?? Boolean(selectedCustomer)}" in app
    assert "window.__delayStatementForCustomer = 'demo-customer-0002'" in visual_review
    assert "A previous customer's statement remained visible" in visual_review
    assert 'f"customer-statement-switch-{SHOT_SUFFIX}.png"' in visual_review


def test_solar_contract_choices_and_history_show_customer_names() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    manager = app.split("function SolarContractManager(", 1)[1].split("\nfunction CustomerStatement(", 1)[0]

    assert "customerLabel(sale.customer_id, customers)" in manager
    assert "<th>Abonné</th><th>Vente</th>" in manager


def test_meter_service_codes_are_localized_for_operators() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert "domainLabel(account.service_code)" in app
    assert "domainLabel(account.connection_status)" in app


def test_business_codes_are_localized_before_they_reach_tables() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    labels = app.split("function domainLabel(", 1)[1].split("\n}\n", 1)[0]

    for code in ("UTILITY_INVOICE", "SOLAR_SALE", "CASH", "BANK", "MOBILE_MONEY", "ELECTRICITY", "WATER", "CONNECTED", "DISCONNECTED"):
        assert f"{code}:" in labels
    assert "domainLabel(row.method)" in app
    assert "domainLabel(row.source_type)" in app


def test_payment_workspace_uses_two_desktop_columns_and_stacks_below_breakpoint() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")

    assert 'className="payment-workspace"' in app
    assert 'title="Récapitulatif"' in app
    assert 'formatMoney(item.balance, item.currency)' in app
    assert 'formatMoney(receivedAmount, currency)' in app
    payment_form = app.split("function PaymentForm(", 1)[1].split("\nfunction MeterReadingsView(", 1)[0]
    assert 'label className="span-two">Abonné<select name="customer_id" value={customerId}' in payment_form
    assert 'name="occurred_at" type="datetime-local"' in payment_form
    assert 'new Date(String(form.get("occurred_at"))).toISOString()' in payment_form
    assert 'label>Référence interne<input value="Générée par Rust à l’enregistrement" readOnly' in payment_form
    assert 'await page.locator(\'input[name="occurred_at"]\').fill("2026-11-04T10:00")' in (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")
    assert ".payment-workspace { display: grid; grid-template-columns: minmax(0, 1.55fr) minmax(285px, .72fr);" in css
    assert "@media (max-width: 1280px)" in css
    assert ".payment-workspace { grid-template-columns: 1fr; }" in css


def test_payment_and_meter_summary_actions_stay_inside_their_panels() -> None:
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")

    assert ".payment-workspace > .panel:last-child, .reading-workspace > .panel:last-child { display: flex; flex-direction: column; }" in css
    assert ".reading-summary, .payment-summary { display: flex; min-height: 0; flex: 1; flex-direction: column; align-items: stretch; gap: 12px; padding: 10px 0; }" in css


def test_new_meter_reading_prefills_last_index_without_recalculating_consumption() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    readings_view = app.split("function MeterReadingsView(", 1)[1].split("\nfunction CustomerStatement(", 1)[0]

    assert 'setPreviousIndex(nextReadings.length ? text(nextReadings[nextReadings.length - 1].current) : "")' in readings_view
    assert 'value={previousIndex}' in readings_view
    assert 'setPreviousIndex(nextReadings.length ? text(nextReadings[nextReadings.length - 1].current) : "")' in readings_view


def test_meter_reading_preview_is_calculated_by_local_rust_before_saving() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    local_api = (ROOT / "v2" / "crates" / "sel-local" / "src" / "lib.rs").read_text(encoding="utf-8")
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")

    assert '"/local/v1/meters/:id/readings/preview"' in local_api
    assert '`${meterBase}/preview`' in app
    assert 'className="reading-workspace"' in app
    assert 'preview.measured_quantity' in app
    assert ".reading-workspace { display: grid; grid-template-columns:" in css
    assert "measured_quantity: current - previous" not in app


def test_meter_reading_period_uses_a_prior_index_and_errors_are_not_success_green() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")
    readings_view = app.split("function MeterReadingsView(", 1)[1].split("\nfunction CustomerStatement(", 1)[0]

    assert 'const preceding = readings.filter((reading) => text(reading.period) < selectedPeriod)' in readings_view
    assert 'className={`reading-message${messageError ? " error" : ""}`}' in readings_view
    assert '.reading-message.error { color: var(--red); }' in css


def test_visual_review_captures_solar_payment_allocation() -> None:
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'f"payments-solar-{SHOT_SUFFIX}.png"' in visual_review
    assert '.payment-workspace form select[name="customer_id"]' in visual_review
    assert 'wait_for(state="attached")' in visual_review
    assert 'select_option("demo-receivable-solar-0001")' in visual_review
    assert 'select_option("demo-solar-schedule-0001")' in visual_review
    assert '"id": "demo-solar-schedule-0001"' in visual_review
    assert "element.scrollIntoView({ block: 'start' })" in visual_review
    assert 'f"solar-contracts-{SHOT_SUFFIX}.png"' in visual_review


def test_visual_review_captures_rust_validated_meter_preview() -> None:
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert "endsWith('/readings/preview')" in visual_review
    assert 'name="period"' in visual_review
    assert 'name="current"' in visual_review
    assert 'name="Prévisualiser les index"' in visual_review
    assert 'get_by_text("100 kWh", exact=True).wait_for()' in visual_review


def test_visual_review_covers_water_meter_and_positive_water_invoice() -> None:
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert '"service_code": "WATER"' in visual_review
    assert '"unit": "M3"' in visual_review
    assert '"demo-meter-water-0001"' in visual_review
    assert 'f"water-reading-preview-{SHOT_SUFFIX}.png"' in visual_review
    assert 'f"invoice-preview-water-positive-{SHOT_SUFFIX}.png"' in visual_review
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    assert 'className="help invoice-preview-status"' in app
    assert "Le calcul et les taxes viennent du moteur Rust." in app
    assert 'get_by_role("heading", name="Prévisualisation de facture").wait_for()' in visual_review
    assert "Number(args.body?.water?.quantity ?? 0)" in visual_review
    assert 'locator(".invoice-preview-metrics").get_by_text("5 m³", exact=True).wait_for()' in visual_review
    assert 'locator(".invoice-preview-metrics").get_by_text("24,36 USD", exact=True).wait_for()' in visual_review
    assert "Ligne de communication" in visual_review
    assert 'await page.get_by_label("Eau consommée (m³)").fill("5")' in visual_review
    assert 'unitCode === "KWH" ? "kWh" : unitCode === "M3" ? "m³"' in (APP / "src" / "App.tsx").read_text(encoding="utf-8")


def test_customer_creation_form_is_revealed_from_the_directory_action() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'customerCreateOpen && <Panel title="Créer un abonné"' in app
    assert 'setCustomerCreateOpen((open) => !open)' in app
    assert '"+ Ajouter un abonné"' in app
    assert 'f"customer-new-{SHOT_SUFFIX}.png"' in visual_review
    assert 'title: "Vérifier le nouvel abonné"' in app
    assert "customerCreateBusy" in app
    assert "Canceling customer creation must not send a write request" in visual_review
    assert "Canceling customer review must preserve the entered customer data" in visual_review
    assert "window.__centralWriteLog.filter(entry => entry.path === '/v1/customers' && entry.method === 'POST').length === 1" in visual_review
    assert 'f"customer-create-confirmation-{SHOT_SUFFIX}.png"' in visual_review


def test_customer_dossier_has_read_only_overview_and_explicit_edit_mode() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")
    dossier = app.split('view === "customers" && (selectedCustomer ?', 1)[1].split("</> : <>", 1)[0]

    assert 'customerEditOpen ?' in dossier
    assert '"Modifier la fiche"' in dossier
    assert '"Annuler la modification"' in dossier
    assert 'className="dossier-metrics metrics compact-metrics"' in app
    assert 'label="Versements" value={rows(statement.payments).length}' in app
    assert 'f"customer-edit-{SHOT_SUFFIX}.png"' in visual_review
    assert 'title: "Vérifier les modifications"' in app
    assert "customerEditBusy" in app
    assert "Customer edits must not be sent before confirmation" in visual_review
    assert "Canceling customer edit review must preserve the entered values" in visual_review
    assert "Canceling customer edit review must not send a write request" in visual_review


def test_service_and_meter_changes_are_reviewed_before_central_writes() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'title: "Vérifier le rattachement du service"' in app
    assert 'title: "Vérifier le numéro du compteur"' in app
    assert 'title: "Vérifier le raccordement"' in app
    assert "Aucun compteur ne sera créé." in app
    assert "Service attachment must not be sent before confirmation" in visual_review
    assert "Canceling service attachment review must preserve the chosen service" in visual_review
    assert "Meter must not be recorded before confirmation" in visual_review
    assert "Canceling meter confirmation must preserve the entered serial" in visual_review
    assert "Service-account edits must wait for confirmation" in visual_review
    assert "Canceling service-account review must preserve the entered state" in visual_review
    assert 'f"meter-create-confirmation-{SHOT_SUFFIX}.png"' in visual_review
    assert 'f"service-account-edit-confirmation-{SHOT_SUFFIX}.png"' in visual_review


def test_solar_contract_is_reviewed_before_creating_installments() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'title: "Vérifier le contrat solaire"' in app
    assert "créance existante" in app
    assert "Aucun intérêt ni frais supplémentaire." in app
    assert "Solar contract must not be created before explicit confirmation" in visual_review
    assert "Canceling solar contract review must preserve the entered terms reference" in visual_review
    assert 'f"solar-contract-confirmation-{SHOT_SUFFIX}.png"' in visual_review


def test_customer_directory_surfaces_truthful_central_metrics_before_the_table() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    directory = app.split('title="Répertoire des abonnés"', 1)[1].split(
        '{view === "readings"', 1
    )[0]

    assert 'className="metrics customer-metrics"' in directory
    assert 'label="Abonnés enregistrés" value={customers.length}' in directory
    assert 'label="Factures" value={invoices.length}' in directory
    assert 'label="Versements" value={payments.length}' in directory
    assert 'value={pending}' not in directory
    assert directory.index('className="metrics customer-metrics"') < directory.index(
        'title="Abonnés enregistrés"'
    )


def test_visual_review_captures_customer_record_and_receivable_statement() -> None:
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert 'f"customer-record-{SHOT_SUFFIX}.png"' in visual_review
    assert 'f"receivables-statement-{SHOT_SUFFIX}.png"' in visual_review
    assert 'get_by_role("button", name="Ouvrir le dossier →")' in visual_review
    assert 'get_by_label("Abonné").select_option("demo-customer-0001")' in visual_review
    assert 'aria-label="Retour au répertoire"' in app
    assert 'view === "customers" && (selectedCustomer ? <>' in app
    assert 'page.get_by_role("heading", name="Dossier abonné")' in visual_review
    assert 'await page.evaluate("window.scrollTo(0, 0)")' in visual_review


def test_solar_contract_visual_evidence_uses_a_desktop_viewport() -> None:
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'path=str(OUTPUT / f"solar-contract-workspace-{SHOT_SUFFIX}.png")' in visual_review
    assert 'path=str(OUTPUT / f"solar-contracts-{SHOT_SUFFIX}.png")' in visual_review
    assert 'await page.evaluate("window.scrollTo(0, 0)")' in visual_review


def test_historical_invoice_view_never_offers_to_issue_an_existing_invoice() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    history_view = app.split("{selectedInvoice &&", 1)[1].split("</>}", 1)[0]

    assert "<Preview data={selectedInvoice}" in history_view
    assert "onConfirm=" not in history_view
    assert "{onConfirm &&" in app


def test_tauri_bridge_fails_closed_and_uses_only_central_http() -> None:
    native = (APP / "src-tauri" / "src" / "lib.rs").read_text(encoding="utf-8")

    assert "start_agent(app.handle(), &data_dir).ok()" not in native
    assert "local_request" not in native
    assert "central_request" in native
    assert 'Command::new("explorer.exe")' in native
    assert 'Command::new("cmd.exe")' not in native
    assert 'Command::new("powershell.exe")' not in native
    assert "https://monarquegrp.com" in native
    assert "open_monarque_site" in native
    assert "open_monarque_site" in (APP / "src" / "api.ts").read_text(encoding="utf-8")


def test_tauri_error_messages_are_preserved_in_ui() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert 'typeof error === "string"' in app
    assert 'const message = (error as { message?: unknown }).message' in app
    assert '"L’opération n’a pas abouti. Vérifiez la connexion au service SEL et réessayez."' in app


def test_central_http_conflicts_are_localized_without_exposing_routes() -> None:
    api = (APP / "src" / "api.ts").read_text(encoding="utf-8")
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'LAST_ADMINISTRATOR: "Impossible de désactiver ou rétrograder le dernier administrateur actif."' in api
    assert 'SELF_MODIFICATION_DENIED: "Un administrateur ne peut pas modifier son propre rôle ou statut."' in api
    assert 'if (status === 403)' in api
    assert 'if (status === 409)' in api
    assert 'window.__rejectDuplicateUser' in visual_review
    assert 'admin-duplicate-user-conflict-' in visual_review
    assert 'admin-self-access-locked-' in visual_review
    assert 'The current administrator must not be able to edit their own role' in visual_review
    assert 'Duplicate identity conflict must be localized without exposing raw API details' in visual_review
    assert '.button:disabled, .button:disabled:hover' in css
    assert 'A disabled primary action must not look active' in visual_review


def test_central_admin_can_bootstrap_before_device_enrollment() -> None:
    bridge = (APP / "src-tauri" / "src" / "lib.rs").read_text(encoding="utf-8")
    api = (APP / "src" / "api.ts").read_text(encoding="utf-8")
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert 'const DEFAULT_CENTRAL_API_URL: &str = "https://sel.shuleops.com"' in bridge
    assert "resolve_central_url(None, None).unwrap()" in bridge
    assert "require_password_change?: boolean" in api
    assert 'values.get("require_password_change") === "on"' in app
    assert 'centralAdminRequest("/v1/auth/change-password", "POST"' in app


def test_central_only_client_does_not_offer_offline_device_activation() -> None:
    bridge = (APP / "src-tauri" / "src" / "lib.rs").read_text(encoding="utf-8")
    api = (APP / "src" / "api.ts").read_text(encoding="utf-8")
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert '["", "v1", "admin", "devices", id, "provision"]' not in bridge
    assert "activate_device" not in api
    assert 'function ServiceAccountsManager' in app
    assert '"Rattacher un service"' in app
    assert '"Ajouter un compteur"' in app
    assert 'Aucun numéro de compteur n’est demandé à cette étape.' in app
    assert 'setActivationUserId' not in app


def test_tauri_windows_bundle_does_not_build_or_ship_the_local_agent() -> None:
    workflow = (ROOT / ".github" / "workflows" / "sel-v2-tauri-windows.yml").read_text(encoding="utf-8")

    assert "sel-local.exe" not in workflow
    assert "cargo build --manifest-path v2/Cargo.toml -p sel-local" not in workflow


def test_figma_visual_review_uses_only_marked_synthetic_fixtures() -> None:
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")
    workflow = (ROOT / ".github" / "workflows" / "sel-v2-tauri-windows.yml").read_text(encoding="utf-8")

    assert '"SEL-DÉMO-0001"' in visual_review
    assert '"Client Alpha"' in visual_review
    assert 'viewport={"width": VIEWPORT_WIDTH, "height": VIEWPORT_HEIGHT}' in visual_review
    assert "document.documentElement.scrollWidth" in visual_review
    assert "SEL-V2-Figma-Desktop-Review-${{ github.run_number }}" in workflow
    assert 'DEMO-SOLAR-TERMS' in visual_review
    assert '"solarPlan": SOLAR_PLAN' in visual_review
    assert "body = fixtures.solarPlan" in visual_review
    assert 'f"solar-contracts-{SHOT_SUFFIX}.png"' in visual_review
    assert 'page.screenshot(path=str(OUTPUT / f"{slug}-{SHOT_SUFFIX}.png"))' in visual_review
