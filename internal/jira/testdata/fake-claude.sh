#!/bin/sh
# Stand-in for `claude -p` in tests: records how it was called and replays a
# canned stream-json file. Never talks to the network.
printf '%s\n' "$@" > "$FAKE_CLAUDE_ARGS"
{ pwd; echo "ENABLE_CLAUDEAI_MCP_SERVERS=$ENABLE_CLAUDEAI_MCP_SERVERS"; } > "$FAKE_CLAUDE_ARGS.env"
if [ -n "$FAKE_CLAUDE_OUT" ]; then cat "$FAKE_CLAUDE_OUT"; fi
exit "${FAKE_CLAUDE_EXIT:-0}"
