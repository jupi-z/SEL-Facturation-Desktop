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


def test_v2_windows_client_keeps_local_api_credentials_out_of_javascript() -> None:
    adapter = (APP / "src" / "api.ts").read_text(encoding="utf-8")
    native = (APP / "src-tauri" / "src" / "lib.rs").read_text(encoding="utf-8")

    assert 'invoke<LocalResponse<T>>("local_request"' in adapter
    assert "Authorization" not in adapter
    assert "/local/v1/" in native
    assert "Bearer " in native
    assert "local_request" in native
    assert '"https://' not in adapter


def test_v2_windows_client_exposes_the_figma_primary_workflows() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    for label in (
        "Vue d’ensemble",
        "Abonnés",
        "Relevés",
        "Factures",
        "Encaissements",
        "Kits solaires",
        "Créances",
        "Synchronisation",
        "Administration",
        "Prévisualiser la facture",
    ):
        assert label in app

    assert 'name="tax_id"' not in app
    assert 'customerCreateOpen && <Panel title="Nouvel abonné"' in app
    assert '"+ Ajouter un abonné"' in app


def test_meter_reading_screen_uses_local_routes_and_displays_rust_measurements() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert '/service-accounts/${encodeURIComponent(accountId)}/meters' in app
    assert '/readings`, "POST"' in app
    assert 'measured_quantity' in app
    assert 'source: "MANUAL"' in app
    assert 'Number(form.get("previous"))' not in app
    assert '/meter-readings/${encodeURIComponent(text(correctingReading.id))}/corrections' in app
    assert "Le relevé initial est conservé." in app
    assert '"readings", "Relevés"' in visual_review
    assert '"DEMO-KWH-001"' in visual_review


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
    assert "height: 93px" in css
    assert "width: 20px; height: 20px; color: #98b9e0" in css


def test_sidebar_uses_distinct_local_icons_for_each_destination() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    icon = app.split("function NavIcon(", 1)[1].split("\n}\n", 1)[0]

    for name in ("grid", "people", "meter", "document", "cash", "sun", "balance", "sync", "settings"):
        assert f"    {name}:" in icon
    assert "void name" not in icon
    assert "stroke=\"currentColor\"" in icon


def test_dashboard_uses_recent_local_operations_without_demo_financial_data() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert 'title="Dossiers / activité récente"' in app
    assert 'headers={["Référence", "Abonné", "Opération", "Montant", "État"]}' in app
    assert "...invoices.map" in app
    assert "...payments.map" in app
    assert "...sales.map" in app
    assert "value={customers.length}" in app
    assert "value={invoices.length}" in app
    assert 'onClick={() => navigateTo("invoices")}>Nouvelle facture' in app
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
    assert "viewport={\"width\": 1440, \"height\": 900}" in visual_review
    assert 'locale="fr-CD"' in visual_review


def test_solar_sales_use_the_approved_local_catalog_and_rust_quote() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    local_api = (ROOT / "v2" / "crates" / "sel-local" / "src" / "lib.rs").read_text(encoding="utf-8")
    storage = (ROOT / "v2" / "crates" / "sel-storage" / "src" / "sqlite.rs").read_text(encoding="utf-8")

    assert '"/local/v1/solar/products"' in local_api
    assert "list_active_solar_products" in storage
    assert '"/local/v1/solar/products"' in app
    assert '"/local/v1/solar/quotes"' in app
    assert '"/local/v1/solar/contracts"' in app
    assert 'local/v1/installment-plans/${encodeURIComponent(planId)}' in app
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
    assert "invoice-preview-1440x900.png" in visual_review
    assert "path === '/local/v1/billing/utility/preview'" in visual_review


def test_invoice_preview_is_a_distinct_figma_aligned_step_before_emission() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")
    invoice_view = app.split('{view === "invoices"', 1)[1].split('{view === "payments"', 1)[0]

    assert 'hidden={Boolean(preview)}' in invoice_view
    assert 'title="Prévisualisation de facture"' in invoice_view
    assert 'className="invoice-preview-metrics metrics"' in invoice_view
    assert 'label="Électricité"' in invoice_view
    assert 'label="Eau"' in invoice_view
    assert 'onClick={() => { setPreview(null); setInvoiceDraft(null); }}' in invoice_view
    assert 'onClick={() => void confirmInvoice()}>Émettre la facture' in invoice_view
    assert 'get_by_role("heading", name="Prévisualisation de facture")' in visual_review
    assert 'get_by_role("button", name="Modifier la saisie")' in visual_review


def test_solar_catalog_is_the_primary_page_and_sale_entry_is_actionable() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'title="Catalogue des kits solaires"' in app
    assert "Créer une vente" in app
    assert 'className="metrics solar-metrics"' in app
    assert 'headers={["SKU", "Produit", "Catégorie", "Prix de référence", "État"]}' in app
    assert "solarSaleOpen &&" in app
    assert ".solar-metrics { grid-template-columns: repeat(4, minmax(0, 1fr));" in css
    assert "solar-1440x900.png" in visual_review
    assert "solar-new-sale-1440x900.png" in visual_review


def test_navigation_clears_transient_notices_from_the_previous_screen() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert "function navigateTo(nextView: View)" in app
    navigation = app.split("function navigateTo(nextView: View)", 1)[1].split("\n  }", 1)[0]
    assert "setNotice(null)" in navigation
    assert "setView(nextView)" in navigation
    assert "onClick={() => navigateTo(item.id)}" in app
    assert 'onClick={() => navigateTo("invoices")}>Nouvelle facture' in app


def test_solar_sales_show_customer_names_instead_of_internal_ids() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert "function customerLabel(customerId: unknown, customers: Row[])" in app
    assert "customerLabel(row.customer_id, customers)" in app
    assert "<SolarContractManager sales={sales} customers={customers}" in app


def test_solar_catalog_route_uses_local_api_error_contract() -> None:
    local_api = (ROOT / "v2" / "crates" / "sel-local" / "src" / "lib.rs").read_text(encoding="utf-8")
    handler = local_api.split("async fn list_solar_products(", 1)[1].split("\n}\n", 1)[0]

    assert "Result<Json<Vec<serde_json::Value>>, (StatusCode, String)>" in handler
    assert ".map_err(internal_error)?" in handler
    assert "ApiError" not in handler
    assert "storage_error" not in handler


def test_solar_payment_form_clears_stale_schedules_and_fails_closed() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    payment_form = app.split("function PaymentForm(", 1)[1].split("\n}\n\nfunction MeterReadingsView", 1)[0]

    assert "setScheduleRows([]); setScheduleId(\"\"); setScheduleCurrency(\"\")" in payment_form
    assert "setScheduleLoading(true)" in payment_form
    assert "setScheduleError(humanError(error))" in payment_form
    assert "disabled={scheduleLoading || Boolean(scheduleError)" in payment_form
    assert "schedule_id: form.get(\"schedule_id\") || null" in payment_form


def test_payment_form_clears_previous_customer_receivables_while_loading() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    payment_form = app.split("function PaymentForm(", 1)[1].split("\n}\n\nfunction MeterReadingsView", 1)[0]

    assert "setReceivables([]); setReceivableId(\"\"); setReceivablesLoading(true)" in payment_form
    assert "disabled={!customerId || receivablesLoading || Boolean(receivablesError) || !receivables.length}" in payment_form
    assert "const paymentBlocked = receivablesLoading || Boolean(receivablesError) || !receivableId" in payment_form
    assert "disabled={paymentBlocked}" in payment_form


def test_windows_typescript_target_does_not_use_es2022_array_at() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert ".at(-1)" not in app
    assert "readings[readings.length - 1]" in app


def test_financial_and_sync_statuses_are_shown_in_french() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert 'LOCAL_PENDING: "En attente de synchronisation"' in app
    assert 'CENTRAL_ACCEPTED: "Confirmé par le serveur"' in app
    assert 'LOCAL_ACCEPTED: "Enregistré sur ce poste"' in app
    assert 'OVERDUE: "En retard"' in app


def test_customer_statement_uses_localized_status_badges() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    statement = app.split("function CustomerStatement(", 1)[1].split("\nfunction AdminCard(", 1)[0]

    assert statement.count("<Status value={item.status} />") == 2
    assert "text(item.status)" not in statement
    assert "customerName: string" in statement
    assert "Situation du compte · ${customerName}" in statement
    assert '<CustomerStatement customerName={text(selectedCustomerData?.legal_name, selectedCustomer)}' in app


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
    assert 'formatMoney(selectedReceivable.balance, selectedReceivable.currency)' in app
    assert 'formatMoney(receivedAmount, currency)' in app
    payment_form = app.split("function PaymentForm(", 1)[1].split("\nfunction MeterReadingsView(", 1)[0]
    assert 'label>Abonné<select name="customer_id" value={customerId}' in payment_form
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

    assert '"payments-solar-1440x900.png"' in visual_review
    assert '.payment-workspace form select[name="customer_id"]' in visual_review
    assert 'wait_for(state="attached")' in visual_review
    assert 'select_option("demo-receivable-solar-0001")' in visual_review
    assert 'select_option("demo-solar-schedule-0001")' in visual_review
    assert '"id": "demo-solar-schedule-0001"' in visual_review
    assert "element.scrollIntoView({ block: 'start' })" in visual_review
    assert '"solar-contracts-1440x900.png"' in visual_review


def test_visual_review_captures_rust_validated_meter_preview() -> None:
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert "endsWith('/readings/preview')" in visual_review
    assert 'name="period"' in visual_review
    assert 'name="current"' in visual_review
    assert 'name="Prévisualiser les index"' in visual_review
    assert 'get_by_text("100 KWH", exact=True).wait_for()' in visual_review


def test_customer_creation_form_is_revealed_from_the_directory_action() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")

    assert 'customerCreateOpen && <Panel title="Nouvel abonné"' in app
    assert 'setCustomerCreateOpen((open) => !open)' in app
    assert '"+ Ajouter un abonné"' in app
    assert '"customer-new-1440x900.png"' in visual_review


def test_customer_dossier_has_read_only_overview_and_explicit_edit_mode() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")
    dossier = app.split('view === "customers" && (selectedCustomer ?', 1)[1].split("</> : <>", 1)[0]

    assert 'customerEditOpen ?' in dossier
    assert '"Modifier la fiche"' in dossier
    assert '"Annuler la modification"' in dossier
    assert 'className="dossier-metrics metrics compact-metrics"' in app
    assert 'label="Versements" value={rows(statement.payments).length}' in app
    assert '"customer-edit-1440x900.png"' in visual_review


def test_customer_directory_surfaces_truthful_local_metrics_before_the_table() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    directory = app.split('title="Répertoire des abonnés"', 1)[1].split(
        '{view === "readings"', 1
    )[0]

    assert 'className="metrics customer-metrics"' in directory
    assert 'label="Abonnés enregistrés" value={customers.length}' in directory
    assert 'label="Factures locales" value={invoices.length}' in directory
    assert 'label="Versements locaux" value={payments.length}' in directory
    assert 'label="À synchroniser" value={pending}' in directory
    assert directory.index('className="metrics customer-metrics"') < directory.index(
        'title="Abonnés enregistrés"'
    )


def test_visual_review_captures_customer_record_and_receivable_statement() -> None:
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert '"customer-record-1440x900.png"' in visual_review
    assert '"receivables-statement-1440x900.png"' in visual_review
    assert 'get_by_role("button", name="Ouvrir le dossier →")' in visual_review
    assert 'get_by_label("Abonné").select_option("demo-customer-0001")' in visual_review
    assert 'aria-label="Retour au répertoire"' in app
    assert 'view === "customers" && (selectedCustomer ? <>' in app
    assert 'page.get_by_role("heading", name="Dossier abonné")' in visual_review
    assert 'await page.evaluate("window.scrollTo(0, 0)")' in visual_review


def test_solar_contract_visual_evidence_uses_a_desktop_viewport() -> None:
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")
    solar_capture = visual_review.split('if slug == "solar":', 1)[1].split(
        'await page.evaluate("window.scrollTo(0, 0)")', 1
    )[0]

    assert "element.scrollIntoView({ block: 'start' })" in solar_capture
    assert 'path=str(OUTPUT / "solar-contracts-1440x900.png")' in solar_capture
    assert "full_page=True" not in solar_capture


def test_historical_invoice_view_never_offers_to_issue_an_existing_invoice() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    history_view = app.split("{selectedInvoice &&", 1)[1].split("</>}", 1)[0]

    assert "<Preview data={selectedInvoice}" in history_view
    assert "onConfirm=" not in history_view
    assert "{onConfirm &&" in app


def test_tauri_bridge_fails_closed_and_offloads_local_http() -> None:
    native = (APP / "src-tauri" / "src" / "lib.rs").read_text(encoding="utf-8")

    assert "start_agent(app.handle(), &data_dir).ok()" not in native
    assert "spawn_blocking" in native
    assert "MAX_LOCAL_RESPONSE_BYTES: usize = 16 * 1024 * 1024" in native
    assert "take((MAX_LOCAL_RESPONSE_BYTES + 1) as u64)" in native
    assert "env_clear()" in native
    assert "https://monarquegrp.com" in native
    assert "open_monarque_site" in native
    assert "open_monarque_site" in (APP / "src" / "api.ts").read_text(encoding="utf-8")


def test_tauri_windows_bundle_creates_sidecar_resource_directory() -> None:
    workflow = (ROOT / ".github" / "workflows" / "sel-v2-tauri-windows.yml").read_text(encoding="utf-8")

    assert "New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination)" in workflow
    assert "Copy-Item -LiteralPath $source -Destination $destination -Force" in workflow


def test_figma_visual_review_uses_only_marked_synthetic_fixtures() -> None:
    visual_review = (APP / "scripts" / "visual_review.py").read_text(encoding="utf-8")
    workflow = (ROOT / ".github" / "workflows" / "sel-v2-tauri-windows.yml").read_text(encoding="utf-8")

    assert '"SEL-DÉMO-0001"' in visual_review
    assert '"Client Alpha"' in visual_review
    assert "viewport={\"width\": 1440, \"height\": 900}" in visual_review
    assert "document.documentElement.scrollWidth" in visual_review
    assert "SEL-V2-Figma-Desktop-Review-${{ github.run_number }}" in workflow
    assert 'DEMO-SOLAR-TERMS' in visual_review
    assert '"solarPlan": SOLAR_PLAN' in visual_review
    assert "body = fixtures.solarPlan" in visual_review
    assert '"solar-contracts-1440x900.png"' in visual_review
    assert 'page.screenshot(path=str(OUTPUT / f"{slug}-1440x900.png"))' in visual_review
