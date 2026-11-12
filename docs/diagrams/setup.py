"""Generates docs/setup.drawio: the whole Kargo lab on one page.

    python3 docs/diagrams/setup.py            # writes docs/setup.drawio
    python3 <skill>/scripts/render.py docs/setup.drawio <out_dir>   # PNG

Needs drawio_kit.py from the drawio-diagram skill; set DRAWIO_KIT to its scripts folder if it moved.
The versions on the workload cards are a snapshot (12 Nov 2026): update VERSIONS when they change.
"""

import os
import sys
from pathlib import Path

KIT = os.environ.get(
    "DRAWIO_KIT",
    str(Path.home() / ".claude/plugins/cache/geba-skills/drawio-diagram/b6067f697dbd/skills/drawio-diagram/scripts"),
)
sys.path.insert(0, KIT)
from drawio_kit import Page, mono, pill, write_drawio  # noqa: E402

# (namespace, what runs there, how to reach it)
VERSIONS = [
    ("helpdesk-dev", "helpdesk <b>0.3.0</b> · 1 pod", "localhost:32080"),
    ("helpdesk-prod", "helpdesk <b>0.3.0</b> · 2 pods", "localhost:32081"),
    ("research-dev", "research <b>0.3.0</b> · 1 pod", "node IP :32082"),
    ("research-test", "research <b>0.3.0</b> · 1 pod", "node IP :32083"),
    ("research-prod", "not promoted yet", "node IP :32084"),
    ("mcp-server-dev", "mcp-server <b>0.2.0</b> · 1 pod", "gateway only"),
    ("mcp-server-prod", "mcp-server <b>0.1.0</b> · 2 pods", "gateway only"),
]

BLUE, ORANGE, GREEN, VIOLET = "#2563EB", "#EA580C", "#059669", "#7C3AED"


def setup() -> Page:
    p = Page("setup", "Setup", 1920, 1520)
    p.title(40, 22, 1100, "Kargo lab · the whole setup",
            "Two Rust agents and an MCP server, each promoted through its own stages by Kargo and Argo CD on one kind cluster")
    p.legend(1180, 34, [("slate", "GitHub / your machine"), ("blue", "Kargo"), ("orange", "Argo CD"),
                        ("green", "Workloads"), ("violet", "AI gateway"), ("azure", "Azure")], per_row=3, col_w=200)

    # ── GitHub ──────────────────────────────────────────────────────────
    gh = p.group(40, 110, 1840, 200, "slate", "GitHub", "gbaeke/kargo-demo (public)")
    tag = p.card(gh, 15, 45, 210, 135, "slate", "1 · You push a tag",
                 f"{mono('git tag research/v0.4.0')}<br>{mono('git push origin …')}<br><br>"
                 "Form: <b>&lt;app&gt;/v&lt;semver&gt;</b>. A push to main builds nothing.")
    act = p.card(gh, 245, 45, 250, 135, "slate", "2 · Actions: release.yaml",
                 "Builds <b>only the tagged app</b> with the one Dockerfile "
                 f"({mono('APP')}, {mono('VERSION')} build args): a static musl binary in a distroless image.")
    ghcr = p.card(gh, 515, 45, 240, 135, "slate", "3 · Images on GHCR",
                  f"{mono('…/kargo-demo/helpdesk')}<br>{mono('…/kargo-demo/research')}<br>"
                  f"{mono('…/kargo-demo/mcp-server')}<br>Public, tag = version. The version is baked in.")
    br = p.card(gh, 775, 45, 330, 135, "slate", "Stage branches · written by Kargo",
                f"{mono('stage/&lt;app&gt;/&lt;stage&gt;')}, one per app and stage (7).<br>"
                "Finished manifests with the image tag filled in: ConfigMap, Service, Deployment. "
                "One commit per promotion. Never edited by hand.")
    p.card(gh, 1125, 45, 700, 135, "slate", "Branch main · yours",
           f"{mono('agents/helpdesk')}, {mono('agents/research')}, {mono('mcp-server')}: Rust code + "
           f"{mono('deploy/base')} and {mono('deploy/stages/&lt;stage&gt;')} (kustomize, image <b>without a tag</b>)<br>"
           f"{mono('kargo/')} pipelines · {mono('argocd/')} ApplicationSet · {mono('agentgateway/')} gateway, model, MCP routes · "
           f"{mono('cluster/')} scripts<br>"
           "Read only by a promotion: a change to a deploy folder takes effect at the next promotion.")
    p.edge(tag, act, "", "exitX=1;exitY=0.5;entryX=0;entryY=0.5;")
    p.edge(act, ghcr, "", "exitX=1;exitY=0.5;entryX=0;entryY=0.5;")

    # ── the cluster ─────────────────────────────────────────────────────
    cl = p.group(40, 350, 1600, 940, "slate", "kind cluster · kargo")

    # Kargo
    kg = p.group(20, 50, 560, 860, "blue", "Kargo", "v1.12.5 · decides what goes where", parent=cl)
    recipe = p.card(kg, 15, 45, 290, 175, "blue", "ClusterPromotionTask · promote-agent",
                    "The one recipe every Stage runs:<br>"
                    "<b>1</b> git-clone main + the stage branch<br><b>2</b> git-clear the stage branch<br>"
                    "<b>3</b> kustomize-set-image: the Freight's tag<br><b>4</b> kustomize-build the stage overlay<br>"
                    "<b>5</b> git-commit · <b>6</b> git-push (retries 3×)<br><b>7</b> argocd-update, wait for Healthy")
    ctrl = p.card(kg, 320, 45, 225, 175, "blue", "Control plane · ns kargo",
                  f"<b>api</b>: UI and CLI on {mono(':31081')}<br>"
                  "<b>controller</b>: asks the registry for new tags every minute, creates Freight, runs promotions<br>"
                  "+ webhooks, management controller")
    p.card(kg, 15, 235, 530, 58, "blue", f"Secret gitops-repo · ns kargo-shared-resources {pill('not in git', 'opt')}",
           "GitHub token for pushing stage branches. Every project finds it by its label and repo URL.")

    def project(y, name, stages):
        card = p.card(kg, 15, y, 530, 110, "blue", f"Project {name} · ns {name}", container=True)
        x, prev = 15, p.node(card, 15, 50, 118, 42, "blue", "<b>Warehouse</b><br>image → Freight", solid=True)
        x += 118
        width = (530 - 30 - 118 - 26 * len(stages)) // len(stages)
        for label in stages:
            node = p.node(card, x + 26, 50, width, 42, "blue", label)
            p.edge(prev, node, parent=card, color=BLUE, style="endSize=5;strokeWidth=1.2;")
            prev, x = node, x + 26 + width
        return card

    project(308, "helpdesk", ["<b>dev</b><br>automatic", "<b>prod</b><br>you promote"])
    research = project(433, "research", ["<b>dev</b><br>automatic", "<b>test</b><br>automatic", "<b>prod</b><br>you promote"])
    project(558, "mcp-server", ["<b>dev</b><br>automatic", "<b>prod</b><br>you promote"])
    p.card(kg, 15, 683, 530, 92, "blue", "How Freight moves",
           "<b>Freight</b> = one image tag, with a two-word alias. A Stage only accepts Freight <b>verified</b> in the "
           "Stage before it: promoted there and its Argo CD Application Healthy. Promoting never changes Freight, "
           "so rolling back is promoting an older one. Auto-promotion is set per Stage in the ProjectConfig.")
    p.card(kg, 15, 790, 530, 52, "blue", "cert-manager · ns cert-manager",
           "Issues the TLS certificates Kargo's internal webhooks need. Nothing else uses it.")

    # Argo CD
    ag = p.group(660, 50, 300, 860, "orange", "Argo CD", "v3.6.1 · applies git", parent=cl)
    argo = p.card(ag, 15, 45, 270, 95, "orange", "ns argocd",
                  f"<b>server</b>: UI on {mono(':31080')}<br><b>repo-server</b>: reads the stage branches<br>"
                  "<b>application-controller</b>: applies them")
    appset = p.card(ag, 15, 155, 270, 105, "orange", "ApplicationSet · agents",
                    "A list of {app, stage} pairs → 7 Applications. Each is annotated "
                    f"{mono('authorized-stage')} for its one Kargo Stage. <b>No auto-sync</b>: Kargo starts every sync.")

    # Workloads
    wl = p.group(980, 50, 300, 860, "green", "Workloads", "ns &lt;app&gt;-&lt;stage&gt;", parent=cl)
    p.card(wl, 15, 45, 270, 95, "green", "Agents · Rust, axum",
           f"One question in, one answer out: {mono('/chat')}. The model may ask for a tool. "
           "<b>No keys</b>: model and tools are reached through the gateway.")
    p.card(wl, 15, 155, 270, 105, "green", "MCP server · Rust, rmcp",
           f"Tools {mono('current_time')} and {mono('magic_8_ball')} (since 0.2.0). "
           "A <b>NetworkPolicy</b> lets only the gateway's proxy pods connect; a direct call from an agent times out.")

    # AI gateway
    gw = p.group(1300, 50, 280, 860, "violet", "AI gateway", "agentgateway v1.7.0", parent=cl)
    p.card(gw, 15, 45, 250, 95, "violet", f"Controller {pill('optional', 'opt')}",
           f"ns agentgateway-system. Installed by {mono('cluster/agentgateway.sh')}, not by Kargo or Argo CD. "
           "Starts the proxy.")
    model = p.card(gw, 15, 155, 250, 105, "violet", "AgentgatewayModel · gpt-6-luna",
                   "The only model served. Provider Azure, Foundry. The API key comes from the "
                   f"Secret {mono('azure')} {pill('not in git', 'opt')} and is added to each request.")
    proxy = p.card(gw, 15, 275, 250, 348, "violet", "Proxy · ns ai-gateway",
                   f"Service {mono('ai-gateway:8080')}, inside the cluster only.<br><br>"
                   "The agents' <b>single way out</b>:<br><br>"
                   f"{mono('/v1/chat/completions')}<br>model requests → Azure<br><br>"
                   f"{mono('/mcp/dev')}, {mono('/mcp/prod')}<br>tool calls → MCP server<br><br>"
                   "dev and test agents use /mcp/dev, prod agents use /mcp/prod.<br><br>"
                   "No caller keys: fine for a lab only.")
    routes = p.card(gw, 15, 635, 250, 132, "violet", "MCP routes",
                    f"HTTPRoute {mono('mcp')} with two AgentgatewayBackends:<br>"
                    f"{mono('/mcp/dev')} → mcp-server-dev<br>{mono('/mcp/prod')} → mcp-server-prod<br>"
                    "Stateless, streamable HTTP, port 8081.")

    # one row per Application: Argo CD node → workload namespace → gateway
    for i, (ns, what, where) in enumerate(VERSIONS):
        app = p.node(ag, 15, 280 + i * 72, 270, 50, "orange", f"<b>Application {ns}</b><br>reads stage/{ns.rsplit('-', 1)[0]}/{ns.rsplit('-', 1)[1]}")
        card = p.card(wl, 15, 275 + i * 72, 270, 60, "green", ns, f"{what} · {mono(where)}")
        p.edge(app, card, "", "exitX=1;exitY=0.5;entryX=0;entryY=0.5;", color=ORANGE)
        if ns.startswith("mcp-server"):
            p.edge(routes, card, "", f"exitX=0;exitY={(30 + (i - 5) * 72) / 132};entryX=1;entryY=0.5;", color=VIOLET)
        else:
            p.edge(card, proxy, "", f"exitX=1;exitY=0.5;entryX=0;entryY={(30 + i * 72) / 348};", color=VIOLET)

    # ── Azure ───────────────────────────────────────────────────────────
    az = p.group(1680, 480, 200, 180, "azure", "Azure AI Foundry")
    azm = p.card(az, 15, 45, 170, 115, "azure", "gpt-6-luna",
                 f"resource {mono('ai-geert')}<br>project {mono('proj-default')}<br><br>"
                 "Tools need reasoning switched off on this API.")

    # ── your machine ────────────────────────────────────────────────────
    me = p.group(40, 1330, 1840, 160, "slate", "Your machine", "everything on 127.0.0.1 · admin / admin")
    p.card(me, 15, 45, 370, 95, "slate", "Browser",
           f"Kargo {mono('localhost:31081')} · Argo CD {mono('localhost:31080')}<br>"
           f"helpdesk {mono(':32080')} dev, {mono(':32081')} prod<br>"
           "research on the node IP until the cluster is recreated")
    p.card(me, 405, 45, 480, 95, "slate", "Commands",
           f"{mono('kargo promote --project &lt;app&gt; --stage prod --freight-alias …')}<br>"
           f"{mono('cluster/cluster.sh up | agents | status | stop | down')}<br>"
           f"{mono('cluster/agentgateway.sh install | check | status')}")
    p.card(me, 905, 45, 450, 95, "slate", f"Secrets {pill('not in git', 'opt')}",
           f"{mono('~/.config/kargo-demo/pat')} → Secret gitops-repo (Kargo pushes with it)<br>"
           f"{mono('~/.config/agentgateway/config.yaml')} → Secret azure (the model key)<br>"
           "Both are piped into the cluster by the scripts.")
    p.card(me, 1375, 45, 450, 95, "slate", "One release, end to end",
           "Tag → image → Warehouse sees it → Freight → dev promotes itself: render, commit to the stage branch, "
           "Argo CD syncs, pods roll. Then you promote to prod. Promote mcp-server before the agents.")

    # ── edges between the big blocks ────────────────────────────────────
    # GHCR → Kargo's controller (straight down)
    p.edge(ghcr, ctrl, "lists tags<br>every minute", "exitX=0.125;exitY=1;entryX=0.911;entryY=0;", color=BLUE, label_x=0.1)
    # recipe → stage branches
    p.edge(recipe, br, "steps 5–6: commit + push", "exitX=0.914;exitY=0;entryX=0.106;entryY=1;",
           points=[(340, 330), (850, 330)], color=BLUE, label_x=-0.1)
    # Argo CD → stage branches (straight up)
    p.edge(argo, br, "reads", "exitX=0.685;exitY=0;entryX=0.258;entryY=1;", color=ORANGE, label_x=0.4)
    # Kargo → Argo CD
    p.edge(research, appset, "step 7: sync to<br>the new commit", "exitX=1;exitY=0.5;entryX=0;entryY=0.5;",
           points=[(660, 888), (660, 607)], color=BLUE, label_x=-0.55)
    # gateway → Azure
    p.edge(model, azm, "HTTPS<br>+ key", "exitX=1;exitY=0.5;entryX=0;entryY=0.5;", color="#0078D4")
    # browser → cluster
    p.edge(me, cl, "NodePorts published on 127.0.0.1", "exitX=0.12;exitY=0;entryX=0.117;entryY=1;", dashed=True)
    return p


if __name__ == "__main__":
    out = Path(__file__).resolve().parent.parent / "setup.drawio"
    write_drawio(str(out), [setup()])
    print(out)
