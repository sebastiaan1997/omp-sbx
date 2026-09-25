# ADR-0001: Make Agent Database Recovery Explicit and Offline

- Status: Superseded
- Date: 2026-09-21
- Superseded by: Per-sandbox OMP state, 2026-09-22

## Context

SQLite locking over a virtiofs mount does not coordinate writers in separate
sandbox microVMs. Sharing host `~/.omp` therefore exposed `agent.db` and every
other OMP SQLite database to cross-VM corruption. Integrity checks and offline
repair tooling treated damage after the fact without removing the unsafe
topology.

## Superseding decision

Each exact sandbox name owns a persistent host `.omp` tree under
`${XDG_STATE_HOME:-$HOME/.local/state}/omp-sbx/sandboxes/<name>/.omp`. The
launcher mounts only that tree and recreates any existing sandbox that lacks
the expected writable mount or still mounts legacy `~/.omp`.

On first use, the state preparer copies regular file-backed legacy state while
excluding SQLite files by name or header, their sidecars, quarantine and repair
artifacts, symlinks, special files, and retired AWS state. Legacy state remains
untouched. Configuration from `~/.omp/agent/config.yml` is a digest-tracked
seed: changed content applies once to each private tree; unchanged content does
not overwrite private edits.

## Consequences

- Concurrent sandboxes never write the same OMP SQLite database.
- `--new` and disposable parallel VM removal retain host-backed OMP state.
- The guest no longer diagnoses or quarantines databases during startup.
- The host CLI no longer exposes `repair-agent-db` or shared database leases.
- Legacy database-backed settings and logins are intentionally not cloned.
- Users can authenticate separately inside private state or use sbx-managed
  provider credentials that keep real secrets host-side.

## Alternatives rejected

- Sharing `~/.omp` with warnings: advisory behavior does not make cross-VM
  SQLite locks safe.
- Guest integrity checks or automatic quarantine: cannot distinguish
  corruption from transient I/O and lock failures safely.
- Copying legacy databases into every private tree: duplicates credentials and
  may propagate an already damaged database.
