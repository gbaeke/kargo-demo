#!/usr/bin/env bash
# The kind cluster for the Kargo lab and what runs on it before any app:
# cert-manager, Argo CD and Kargo.
#
#   cluster/cluster.sh up        create the cluster (or start it) and install the platform
#   cluster/cluster.sh platform  install or upgrade cert-manager, Argo CD and Kargo
#   cluster/cluster.sh stop      stop the node container; state is kept
#   cluster/cluster.sh status    pods of the platform, Applications and Stages
#   cluster/cluster.sh down      delete the cluster
#
# Logins are admin / $ADMIN_PASSWORD (default: admin) for both UIs. They are
# only reachable on 127.0.0.1 (see kind.yaml).
set -euo pipefail

CLUSTER=kargo
CONTEXT="kind-$CLUSTER"
DIR="$(dirname "$(realpath "$0")")"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin}"

CERT_MANAGER_VERSION=1.21.3
ARGOCD_CHART_VERSION=10.18.0 # Argo CD v3.6.1
KARGO_VERSION=1.12.5

k() { kubectl --context "$CONTEXT" "$@"; }
h() { helm --kube-context "$CONTEXT" "$@"; }
exists() { kind get clusters 2>/dev/null | grep -qx "$CLUSTER"; }

up() {
  if exists; then
    docker start "$CLUSTER-control-plane" >/dev/null
  else
    kind create cluster --name "$CLUSTER" --config "$DIR/kind.yaml"
  fi
  k wait --for=condition=Ready nodes --all --timeout=180s
  platform
}

platform() {
  command -v kargo >/dev/null || { echo "the kargo CLI is needed (to hash the admin password)" >&2; exit 1; }

  h upgrade -i cert-manager cert-manager --repo https://charts.jetstack.io \
    --version "$CERT_MANAGER_VERSION" -n cert-manager --create-namespace \
    --set crds.enabled=true --wait

  # Plain http and no SSO: a lab on localhost.
  h upgrade -i argocd argo-cd --repo https://argoproj.github.io/argo-helm \
    --version "$ARGOCD_CHART_VERSION" -n argocd --create-namespace \
    --set-string "configs.secret.argocdServerAdminPassword=$(argocd account bcrypt --password "$ADMIN_PASSWORD")" \
    --set-string configs.params.server\\.insecure=true \
    --set dex.enabled=false --set notifications.enabled=false \
    --set server.service.type=NodePort --set server.service.nodePortHttp=31080 \
    --wait --timeout 5m

  # The signing key is kept across upgrades so logins stay valid.
  signing_key="$(k -n kargo get secret kargo-api -o jsonpath='{.data.ADMIN_ACCOUNT_TOKEN_SIGNING_KEY}' 2>/dev/null | base64 -d || true)"
  [ -n "$signing_key" ] || signing_key="$(openssl rand -hex 24)"
  # Warehouses poll every 5 minutes at least by default; 1 minute suits a lab.
  h upgrade -i kargo oci://ghcr.io/akuity/kargo-charts/kargo \
    --version "$KARGO_VERSION" -n kargo --create-namespace \
    --set api.service.type=NodePort --set api.service.nodePort=31081 \
    --set api.tls.enabled=false \
    --set-string "api.adminAccount.passwordHash=$(kargo hash-password --password "$ADMIN_PASSWORD")" \
    --set-string "api.adminAccount.tokenSigningKey=$signing_key" \
    --set controller.reconcilers.warehouses.minReconciliationInterval=1m \
    --wait --timeout 5m

  echo
  echo "Argo CD: http://localhost:31080   Kargo: http://localhost:31081   (admin / $ADMIN_PASSWORD)"
}

stop() {
  docker stop "$CLUSTER-control-plane" >/dev/null
  echo "cluster $CLUSTER stopped"
}

status() {
  k get pods -n cert-manager
  k get pods -n argocd
  k get pods -n kargo
  echo
  k get applications -n argocd 2>/dev/null || true
  k get warehouses,stages,freight -A 2>/dev/null || true
}

down() { kind delete cluster --name "$CLUSTER"; }

case "${1:-}" in
  up|platform|stop|status|down) "$1" ;;
  *) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 1 ;;
esac
