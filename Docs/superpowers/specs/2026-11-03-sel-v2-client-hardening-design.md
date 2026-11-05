# SEL V2 Client Hardening and Invoice Preview Design

## Intent

Continue the delivered SEL V2 client without replacing its Python/pywebview/Rust architecture. The first increment removes unsafe HTML interpolation from the native client and makes invoice creation an explicit preview-then-confirm flow. This protects customer-originated data and prevents a preview action from silently creating a definitive financial document.

## Goals

- Render customer, invoice, payment, solar and synchronization data without inserting untrusted values as HTML.
- Preserve the local bridge boundary: JavaScript can call only `/local/v1/` routes through pywebview.
- Add a local Rust preview route that reuses the existing tariff lookup and billing engine but never writes invoices, receivables, journal events or outbox rows.
- Let the operator inspect both service lines, taxes, subtotal and total before explicit emission.
- Keep the existing issue endpoint as the only route that persists a definitive invoice.
- Add regression tests for hostile display strings, the preview non-persistence invariant, and the explicit confirmation path.

## Non-goals

- No new financial calculation in JavaScript.
- No PDF storage or server-side document generation.
- No redesign of the whole V2 navigation in this increment.
- No migration or deletion of existing SQLite data.
- No change to tariff values, taxes, identifiers, synchronization protocol or historical invoices.

## Constraints

- Rust remains the financial authority.
- A combined invoice always has electricity and water lines; zero water remains visible.
- Existing local authentication and pywebview route allow-list remain enforced.
- Changes are additive and must preserve the `794679b` candidate as a rollback point.
- The preview response is explicitly non-persistent and carries `LOCAL_PREVIEW` status.

## Approaches considered

1. **Patch only the JavaScript strings.** Smallest diff, but leaves the invoice button persisting immediately and cannot prove a preview is non-financial.
2. **Add a local preview route and safe DOM rendering (chosen).** Reuses the existing Rust calculator and tariff repository, isolates the state transition in the UI, and gives tests a clear non-persistence boundary.
3. **Replace the V2 client with a framework.** Unnecessary dependency and packaging risk; contrary to the requirement to continue the existing application.

The second approach is selected because it fixes the two P0 client defects with the smallest architecture-preserving change.

## Design decisions

- `POST /local/v1/billing/utility/preview` accepts the same `UtilityBillingRequest` as issue and returns `UtilityBillingResponse` with `status: LOCAL_PREVIEW`, a generated preview invoice id, no receivable id, no invoice number, and the canonical snapshot hash.
- The route loads the approved tariff snapshot through the existing local store and calls `sel_domain::billing::calculate_utility` exactly as the issue route does.
- Preview is not written to SQLite. The definitive issue request is sent only from the confirmation control and remains the existing persistence route.
- JavaScript rendering uses element creation and `textContent`; `innerHTML` is reserved only for fixed, developer-authored markup if any remains after the change. User-provided values never reach an HTML parser.
- The preview panel is keyboard reachable, shows both services and totals, and exposes `Émettre la facture` only after a successful preview.

## Validation boundary

The increment is complete only when targeted Rust tests, Python/static client tests, and the full available Python suite pass. Windows packaging and PostgreSQL remain separate validation gates and are reported independently.
