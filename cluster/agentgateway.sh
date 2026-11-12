#!/usr/bin/env bash
# Optional: agentgateway (https://agentgateway.dev) in Kubernetes mode on the
# lab cluster, with one Azure model. Installed straight with Helm and kubectl,
# not through Argo CD or Kargo.
#
#   cluster/agentgateway.sh install     controller, the gateway, the Azure key, the model and the MCP routes
#   cluster/agentgateway.sh check       one chat request through the gateway
#   cluster/agentgateway.sh status      controller, gateway and models
#   cluster/agentgateway.sh uninstall   remove the gateway, the key and the controller
#
# The Azure API key is copied from the standalone agentgateway on this machine
# ($AGENTGATEWAY_CONFIG, the model named $SOURCE_MODEL) into a Secret in the
# cluster. It goes from that file to kubectl over a pipe: never into git, a
# file in this repo or a command line.
set -euo pipefail

CLUSTER=kargo
CONTEXT="kind-$CLUSTER"
DIR="$(dirname "$(realpath "$0")")"
MANIFESTS="$DIR/../agentgateway"

GATEWAY_API_VERSION=v1.6.3
AGENTGATEWAY_VERSION=v1.7.0
CHARTS=oci://cr.agentgateway.dev/charts
SYSTEM_NAMESPACE=agentgateway-system
NAMESPACE=ai-gateway

AGENTGATEWAY_CONFIG="${AGENTGATEWAY_CONFIG:-$HOME/.config/agentgateway/config.yaml}"
SOURCE_MODEL="${SOURCE_MODEL:-azure-gpt-6-luna}" # its name in that config
MODEL=gpt-6-luna                                 # its name in the cluster (agentgateway/models.yaml)

k() { kubectl --context "$CONTEXT" "$@"; }
h() { helm --kube-context "$CONTEXT" "$@"; }

# The Secret "azure", in the form agentgateway expects: one entry named
# Authorization holding the key.
key_secret() {
  command -v yq >/dev/null || { echo "yq is needed to read $AGENTGATEWAY_CONFIG" >&2; exit 1; }
  [ -f "$AGENTGATEWAY_CONFIG" ] || { echo "no agentgateway config at $AGENTGATEWAY_CONFIG" >&2; exit 1; }
  local key
  key="$(name="$SOURCE_MODEL" yq -r '.llm.models[] | select(.name == strenv(name)) | .params.apiKey // ""' "$AGENTGATEWAY_CONFIG")"
  [ -n "$key" ] || { echo "no apiKey for model $SOURCE_MODEL in $AGENTGATEWAY_CONFIG" >&2; exit 1; }
  # printf is a shell builtin, so the key doesn't show up in ps.
  printf 'Authorization=%s\n' "$key" |
    k -n "$NAMESPACE" create secret generic azure --from-env-file=/dev/stdin --dry-run=client -o yaml |
    k apply -f - >/dev/null
  echo "secret/azure set from $SOURCE_MODEL in $AGENTGATEWAY_CONFIG"
}

install() {
  k apply --server-side -f \
    "https://github.com/kubernetes-sigs/gateway-api/releases/download/$GATEWAY_API_VERSION/standard-install.yaml" >/dev/null
  h upgrade -i --create-namespace -n "$SYSTEM_NAMESPACE" --version "$AGENTGATEWAY_VERSION" \
    agentgateway-crds "$CHARTS/agentgateway-crds" >/dev/null
  h upgrade -i -n "$SYSTEM_NAMESPACE" --version "$AGENTGATEWAY_VERSION" \
    agentgateway "$CHARTS/agentgateway" --wait >/dev/null
  k apply -f "$MANIFESTS/gateway.yaml"
  key_secret
  k apply -f "$MANIFESTS/models.yaml"
  k apply -f "$MANIFESTS/mcp.yaml"
  k -n "$NAMESPACE" wait --for=condition=Programmed gateway/ai-gateway --timeout=120s
  k -n "$NAMESPACE" rollout status deploy/ai-gateway --timeout=120s
  status
}

# The gateway has no port outside the cluster, so this forwards one for the
# length of the request. kubectl is started directly, not through k(), so
# that $! is kubectl itself and the trap really stops it.
check() {
  local port=18080 pid
  kubectl --context "$CONTEXT" -n "$NAMESPACE" port-forward svc/ai-gateway "$port:8080" >/dev/null &
  pid=$!
  trap "kill $pid 2>/dev/null" EXIT
  for _ in $(seq 20); do curl -s -m 2 -o /dev/null "http://127.0.0.1:$port/" && break; sleep 0.5; done
  echo "asking $MODEL to reply with: the gateway works"
  curl -sS -m 60 "http://127.0.0.1:$port/v1/chat/completions" -H 'content-type: application/json' \
    -d "{\"model\": \"$MODEL\", \"messages\": [{\"role\": \"user\", \"content\": \"Reply with exactly: the gateway works\"}]}" |
    jq -r '.choices[0].message.content // .'
}

status() {
  k -n "$SYSTEM_NAMESPACE" get pods
  echo
  k -n "$NAMESPACE" get gateway,agentgatewaymodels,agentgatewaybackends,httproutes,pods,services
}

uninstall() {
  k delete namespace "$NAMESPACE" --ignore-not-found
  h uninstall -n "$SYSTEM_NAMESPACE" agentgateway --ignore-not-found
  h uninstall -n "$SYSTEM_NAMESPACE" agentgateway-crds --ignore-not-found
  k delete namespace "$SYSTEM_NAMESPACE" --ignore-not-found
  # The Gateway API CRDs stay: other controllers may use them.
}

case "${1:-}" in
  install|check|status|uninstall) "$1" ;;
  *) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 1 ;;
esac
