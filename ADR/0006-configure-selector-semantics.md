# ADR-0006: Keep Configure Selectors and Storage Values Typed

- Status: Accepted
- Date: 2026-09-21

## Context

The configure workflow sends model selectors and configuration values through `omp config set`. These arguments do not all use the same encoding:

- `modelRoles` and `task.agentModelOverrides` accept JSON objects.
- `modelRoleStorage` accepts the raw enum token `global` or `project`.
- Model selectors may contain a thinking suffix such as `:low` or `:off`.

Encoding every value as JSON caused the enum command to receive `"project"` instead of `project`, which the guest rejected even though the JSON representation was valid.

## Decision

The Rust configure command preserves the external command's argument types:

- Encode object-valued configuration as JSON.
- Pass `modelRoleStorage` as an unquoted enum token.
- Validate selectors against the discovered catalog before writes.
- Validate thinking suffixes against the selected model's advertised capabilities.
- Use native inheritance for bare selectors and `:inherit`.
- Preserve `:off` as an explicit disabled-thinking request.

## Consequences

- Rust and shell configure commands produce compatible guest configuration.
- Debug output exposes the exact argument shape sent to `omp config set`.
- Tests must cover both JSON-valued writes and raw enum writes; a syntactically valid JSON value is not sufficient evidence of command compatibility.
- Adding another configuration key requires deciding its external argument type explicitly rather than applying one generic serializer.
