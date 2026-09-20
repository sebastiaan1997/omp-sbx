#!/usr/bin/env sh
set -eu

# Docker Sandboxes may append agent or MCP arguments to the configured
# entrypoint. This configure-only container intentionally ignores them and stays
# alive solely for noninteractive sbx exec commands.
exec sleep infinity
