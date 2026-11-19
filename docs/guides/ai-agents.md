# Connecting an AI agent (MCP)

Back to the [README](../../README.md).

The server answers the Model Context Protocol at `/mcp`. Create an access token in the
workbench (Settings → Account → Access tokens), then in Claude Code:

```bash
claude mcp add --transport http mythos http://localhost:8080/mcp \
  --header "Authorization: Bearer mythos_pat_..."
```

The agent then has a tool for every route: projects, field I/O configuration and status, tag
values with their quality, modules, HMI screens, data types, faceplates, and accounts. It acts
with the token owner's role: a tool the role does not allow answers 403. It refuses requests
whose `Origin` is a web page on another host.

Every route of the HTTP API is a tool, or is listed in `NOT_TOOLS` in
[`crates/mythos-core/src/mcp.rs`](../../crates/mythos-core/src/mcp.rs) with the reason it cannot
be one (the telemetry stream, static files). A test reads the router and fails on a route that is
neither. Each route a service module declares (HMI, logic, discovery) is a tool of its own,
listed by `tools/list`.
