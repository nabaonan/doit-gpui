## Agent skills

### Issue tracker

Issues live as GitHub issues; use the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Five canonical triage roles, kept as the defaults: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. `/wayfinder` also uses `wayfinder:map` and the four `wayfinder:<type>` labels (`research`/`prototype`/`grilling`/`task`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` + `docs/adr/` at the repo root; skills read them before exploring and proceed silently when absent. See `docs/agents/domain.md`.
