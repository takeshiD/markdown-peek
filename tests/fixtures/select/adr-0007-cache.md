# ADR 0007: Use an in-memory cache for selection results

## Status

Accepted

## Context

Selection decisions are recomputed on every file save. Most sections do not change between saves.

## Decision

We will cache decisions in memory keyed by the section content hash. We will not persist them to disk.

## Consequences

Restarting the server loses the cache. Memory use is bounded by an LRU limit of 256 entries.
