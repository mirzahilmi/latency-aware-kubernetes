# see https://demontalembert.com/colours-for-makefile/
COLOUR_GREEN=\033[0;32m
COLOUR_RED=\033[0;31m
COLOUR_BLUE=\033[0;34m
COLOUR_END=\033[0m

.PHONY: all
all:
	make cluster
	make namespace
	make otelcol
	make agent
	make stack

.PHONY: bye
bye:
ifeq ($(remote), 1)
	@printf "$(COLOUR_BLUE)> %s$(COLOUR_END)\n" "Truncating resource on remote cluster is not provided"
endif
	make cluster.rm

.PHONY: cluster
cluster:
ifneq ($(remote), 1)
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Pulling kind docker image"
	docker pull kindest/node:v1.34.0
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Creating kind cluster"
	kind create cluster --image kindest/node:v1.34.0 --config ./k8s/Cluster.yaml
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Loading netshoot image for network debugging"
	docker pull bretfisher/netshoot:latest
	kind load docker-image bretfisher/netshoot:latest
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Patching kind cluster to include metrics-server"
	kubectl apply \
		--filename https://github.com/kubernetes-sigs/metrics-server/releases/download/v0.8.0/components.yaml
	kubectl patch -n kube-system deployment metrics-server --type=json \
		-p '[{"op":"add","path":"/spec/template/spec/containers/0/args/-","value":"--kubelet-insecure-tls"}]'
else
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Skipping cluster creation on remote cluster"
endif

.PHONY: namespace
namespace:
	kubectl apply --filename ./k8s/Namespace.yaml

.PHONY: cluster.rm
cluster.rm:
ifneq ($(remote), 1)
	kind delete cluster
else
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Skipping cluster removal on remote cluster"
endif

.PHONY: stack
stack:
ifneq ($(remote), 1)
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Pulling & loading hellopod image ahead"
	docker pull ghcr.io/mirzahilmi/hellopod:0.1.1
	kind load docker-image ghcr.io/mirzahilmi/hellopod:0.1.1
	docker pull ghcr.io/mirzahilmi/prober:0.2.6
	kind load docker-image ghcr.io/mirzahilmi/prober:0.2.6
endif
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Applying resources"
	kubectl apply --filename ./k8s/Hellopod.yaml

.PHONY: stack.rm
stack.rm:
	kubectl delete --ignore-not-found=true \
		--filename ./k8s/Hellopod.yaml

.PHONY: agent
agent:
ifneq ($(remote), 1)
else
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Applying resources"
	kubectl apply \
		--filename ./k8s/Prober.yaml \
		--filename ./k8s/HttpRater.yaml
endif

.PHONY: agent.rm
agent.rm:
ifneq ($(remote), 1)
else
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Deleting resources"
	kubectl delete \
		--filename ./k8s/Prober.yaml \
		--filename ./k8s/HttpRater.yaml
endif

.PHONY: otelcol
otelcol:
	kubectl apply --filename ./k8s/Secret.yaml
	helm repo add open-telemetry https://open-telemetry.github.io/opentelemetry-helm-charts
	helm install opentelemetry-collector open-telemetry/opentelemetry-collector \
		--namespace riset \
		--values ./k8s/chart-values/opentelemetry_collector.yaml

.PHONY: otelcol.re
otelcol.re:
	helm upgrade opentelemetry-collector open-telemetry/opentelemetry-collector \
		--namespace riset \
		--reuse-values \
		--values ./k8s/chart-values/opentelemetry_collector.yaml

.PHONY: otelcol.rm
otelcol.rm:
	helm uninstall opentelemetry-collector --namespace riset --ignore-not-found

