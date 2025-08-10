# see https://demontalembert.com/colours-for-makefile/
COLOUR_GREEN=\033[0;32m
COLOUR_RED=\033[0;31m
COLOUR_BLUE=\033[0;34m
COLOUR_END=\033[0m

.PHONY: all
all:
	make cluster
	make cilium
	make stack

.PHONY: bye
bye:
	make cluster.rm

.PHONY: cluster
cluster:
ifneq ($(remote), 1)
	kind create cluster --config ./k8s/Cluster.yaml
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Loading netshoot image for network debugging"
	docker pull bretfisher/netshoot:latest
	kind load docker-image bretfisher/netshoot:latest
else
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Skipping cluster creation on remote cluster"
endif

.PHONY: cluster.rm
cluster.rm:
	kind delete cluster

.PHONY: cilium
cilium:
ifneq ($(remote), 1)
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Adding cilium into helm repo"
	helm repo add cilium https://helm.cilium.io/
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Pulling & loading cilium image ahead"
	docker pull quay.io/cilium/cilium:v1.17.6
	kind load docker-image quay.io/cilium/cilium:v1.17.6
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Installing cilium chart"
	helm install cilium cilium/cilium --version 1.17.6 \
		--namespace kube-system \
		--values ./k8s/chart-values/cilium.yaml \
		--values ./k8s/chart-values/cilium.kind.yaml
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Waiting for cilium to be healthy"
	cilium status --wait --wait-duration 10m15s
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Pulling & loading prometheus w/ grafana image ahead"
	docker pull prom/prometheus:v2.42.0
	docker pull docker.io/grafana/grafana:9.3.6
	kind load docker-image prom/prometheus:v2.42.0
	kind load docker-image docker.io/grafana/grafana:9.3.6
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Installing prometheus w/ grafana workload"
	kubectl apply --filename https://raw.githubusercontent.com/cilium/cilium/1.17.6/examples/kubernetes/addons/prometheus/monitoring-example.yaml
else
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Adding cilium into helm repo"
	helm --kubeconfig ./kubeconfig.yaml repo add cilium https://helm.cilium.io/
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Installing cilium chart"
	helm --kubeconfig ./kubeconfig.yaml install cilium cilium/cilium --version 1.17.6 \
		--namespace kube-system \
		--values ./k8s/chart-values/cilium.yaml
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Waiting for cilium to be healthy"
	cilium --kubeconfig ./kubeconfig.yaml status --wait --wait-duration 10m15s
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Installing prometheus w/ grafana workload"
	kubectl --kubeconfig ./kubeconfig.yaml apply \
		--filename https://raw.githubusercontent.com/cilium/cilium/1.17.6/examples/kubernetes/addons/prometheus/monitoring-example.yaml \
		--filename ./k8s/Cilium.yaml
endif

.PHONY: cilium.update
cilium.update:
ifneq ($(remote), 1)
	helm upgrade cilium cilium/cilium \
		--namespace kube-system \
		--reuse-values \
		--values ./k8s/chart-values/cilium.yaml \
		--values ./k8s/chart-values/cilium.kind.yaml
else
	helm --kubeconfig ./kubeconfig.yaml upgrade cilium cilium/cilium \
		--namespace kube-system \
		--reuse-values \
		--values ./k8s/chart-values/cilium.yaml
endif

.PHONY: cilium.rm
cilium.rm:
ifneq ($(remote), 1)
	helm uninstall cilium --namespace kube-system
	kubectl delete \
		--filename https://raw.githubusercontent.com/cilium/cilium/1.17.6/examples/kubernetes/addons/prometheus/monitoring-example.yaml \
		--filename ./k8s/Cilium.yaml
else
	helm --kubeconfig ./kubeconfig.yaml uninstall cilium --namespace kube-system
	kubectl --kubeconfig ./kubeconfig.yaml delete \
		--filename https://raw.githubusercontent.com/cilium/cilium/1.17.6/examples/kubernetes/addons/prometheus/monitoring-example.yaml \
		--filename ./k8s/Cilium.yaml
endif

.PHONY: stack
stack:
ifneq ($(remote), 1)
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Pulling & loading hellopod image ahead"
	docker pull ghcr.io/mirzahilmi/hellopod:0.1.1
	kind load docker-image ghcr.io/mirzahilmi/hellopod:0.1.1
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Applying resources"
	kubectl apply \
		--filename ./k8s/Namespace.yaml \
		--filename ./k8s/Hellopod.yaml
else
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Applying resources"
	kubectl --kubeconfig ./kubeconfig.yaml apply \
		--filename ./k8s/Namespace.yaml \
		--filename ./k8s/Hellopod.yaml
endif

.PHONY: stack.rm
stack.rm:
ifneq ($(remote), 1)
	kubectl delete \
		--filename ./k8s/Namespace.yaml \
		--filename ./k8s/Hellopod.yaml
else
	kubectl --kubeconfig ./kubeconfig.yaml delete \
		--filename ./k8s/Namespace.yaml \
		--filename ./k8s/Hellopod.yaml
endif

.PHONY: load
load:
	HELLOPOD_HOSTNAME=172.18.0.3 k6 run ./traffic/load_test.js
