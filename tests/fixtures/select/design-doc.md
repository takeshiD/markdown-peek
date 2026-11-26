# Payment Service Design

## Overview

The payment service accepts card payments and settles them nightly. It replaces the legacy monolith endpoint.

## Architecture

Requests enter through the API gateway, are validated, and are written to an outbox table. A worker publishes outbox rows to the ledger.

## Components

| Component | Owner | Status |
|---|---|---|
| Gateway | platform | done |
| Validator | payments | in progress |
| Outbox worker | payments | todo |
| Ledger adapter | finance | todo |

## Risks

> [!WARNING]
> Double charging is possible if the outbox worker retries without an idempotency key.

## Open Questions

- Should refunds go through the same outbox?
- Who owns the reconciliation report?
