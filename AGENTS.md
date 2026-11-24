## 接手必读

接手项目请先阅读 [AGENT.md](./AGENT.md)：它包含架构地图、SnapshotV2 数据契约、同步与合并语义、颜色规范、GPUI 平台陷阱和发版路径。用户视角说明见 [README.md](./README.md)。

## Agent skills

### Issue tracker

Issues live as GitHub issues; use the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Five canonical triage roles, kept as the defaults: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. `/wayfinder` also uses `wayfinder:map` and the four `wayfinder:<type>` labels (`research`/`prototype`/`grilling`/`task`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` + `docs/adr/` at the repo root; skills read them before exploring and proceed silently when absent. See `docs/agents/domain.md`.
