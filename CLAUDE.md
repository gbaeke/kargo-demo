# CLAUDE.md

A lab for learning Kargo: two Rust agents (`helpdesk`, `research`) and an MCP server, each promoted through its own stages by Kargo + Argo CD on a local kind cluster. Repo: `gbaeke/kargo-demo` (public).

## Layout

- `agents/<name>/` — an agent: `src/` and `deploy/{base,stages/<stage>}` (kustomize).
- `mcp-server/` — same shape, one demo tool (`current_time`).
- `kargo/shared.yaml` — the one `ClusterPromotionTask` (`promote-agent`) every Stage uses.
- `kargo/<app>.yaml` — per app: Project, ProjectConfig, Warehouse, Stages.
- `argocd/agents.yaml` — ApplicationSet; one line per app and stage.
- `agentgateway/` — optional AI gateway: Gateway, Azure model, MCP routes.
- `cluster/cluster.sh` (`up`, `agents`, `status`, `stop`, `down`), `cluster/agentgateway.sh` (`install`, `check`, `status`, `uninstall`).
- `docs/multi-cluster.md` — how stages on separate clusters would work (not built).

## Conventions the pipeline relies on

- Kargo project name = app name. From it the recipe derives the image `ghcr.io/gbaeke/kargo-demo/<app>`, the folder `agents/<app>/deploy` (mcp-server overrides the `deploy` var), the branch `stage/<app>/<stage>` and the Argo CD Application `<app>-<stage>`.
- Namespaces: `<app>` holds Kargo objects, `<app>-<stage>` holds the workload.
- Images have no tag on `main`; Kargo sets it at promotion. Never edit `stage/*` branches.
- A release is a tag `<app>/v<semver>` (e.g. `research/v0.4.0`); the workflow builds only that app.
- Pipelines: helpdesk and mcp-server dev → prod; research dev → test → prod. Prod is manual and left to the user.
- Agents reach the model and the MCP server only through the gateway (`/v1/chat/completions`, `/mcp/dev`, `/mcp/prod`). A NetworkPolicy on the MCP server blocks everything else.

## Working here

- Cluster context `kind-kargo`. UIs: Argo CD `localhost:31080`, Kargo `localhost:31081` (admin/admin, localhost only).
- Agent URLs: `cluster/cluster.sh status`. Ports above 32081 are only on the node IP until the cluster is recreated.
- Changes to `deploy/` on `main` take effect at the next promotion, not on push.
- After changing `kargo/` or `argocd/`: `cluster/cluster.sh agents`. After changing `agentgateway/`: `cluster/agentgateway.sh install`.
- `gpt-6-luna` needs `reasoning_effort: none` when tools are sent on chat completions.
- Avoid `pkill -f` on patterns that match the running shell command.

## Secrets (never in git)

- GitHub PAT for Kargo: `~/.config/kargo-demo/pat` → Secret `gitops-repo` in `kargo-shared-resources`.
- Azure key: read from `~/.config/agentgateway/config.yaml` by `agentgateway.sh` → Secret `azure` in `ai-gateway`.
- Before committing, check staged changes for both values.

## Manual

Explainer artifact (private), update in place when the lab changes: https://claude.ai/artifact/93mnZapt2U1KQYJmb7QMXG
Source: built in the session scratchpad; read the artifact to get the current HTML before editing.
