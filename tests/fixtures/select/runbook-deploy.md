# Deploy runbook

## Prerequisites

- [ ] VPN connected
- [ ] On-call engineer notified
- [ ] Change ticket approved

## Steps

1. Freeze merges to `main`.
2. Tag the release commit.
3. Run the deploy pipeline.
4. Watch the error-rate dashboard for 15 minutes.
5. Unfreeze merges.

## Commands

```bash
git tag -a v2.3.0 -m "release"
git push origin v2.3.0
./scripts/deploy.sh production
```

## Rollback

> [!CAUTION]
> Rolling back the database migration deletes data written after the deploy.

## Contacts

Ask in #deploys.
