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
	make prometheus

.PHONY: bye
bye:
	make cluster.rm
	make prometheus.rm
	make stack.rm
	make cilium.rm

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
ifneq ($(remote), 1)
	kind delete cluster
else
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Skipping cluster removal on remote cluster"
endif

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
else
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Adding cilium into helm repo"
	helm --kubeconfig ./kubeconfig.yaml repo add cilium https://helm.cilium.io/
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Installing cilium chart"
	helm --kubeconfig ./kubeconfig.yaml install cilium cilium/cilium --version 1.17.6 \
		--namespace kube-system \
		--values ./k8s/chart-values/cilium.yaml
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Waiting for cilium to be healthy"
	cilium --kubeconfig ./kubeconfig.yaml status --wait --wait-duration 10m15s
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
else
	helm --kubeconfig ./kubeconfig.yaml uninstall cilium --namespace kube-system
endif

.PHONY: stack
stack:
ifneq ($(remote), 1)
	@printf "$(COLOUR_BLUE)> %s...$(COLOUR_END)\n" "Pulling & loading hellopod image ahead"
	docker pull ghcr.io/mirzahilmi/hellopod:0.1.1
	kind load docker-image ghcr.io/mirzahilmi/hellopod:0.1.1
	docker pull ghcr.io/mirzahilmi/prober:0.2.6
	kind load docker-image ghcr.io/mirzahilmi/prober:0.2.6
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
		--ignore-not-found=true \
		--filename ./k8s/Namespace.yaml \
		--filename ./k8s/Hellopod.yaml
else
	kubectl --kubeconfig ./kubeconfig.yaml delete \
		--ignore-not-found=true \
		--filename ./k8s/Namespace.yaml \
		--filename ./k8s/Hellopod.yaml
endif

.PHONY: prometheus
prometheus:
ifneq ($(remote), 1)
	helm install prometheus oci://ghcr.io/prometheus-community/charts/prometheus \
		--create-namespace \
		--namespace prometheus \
		--values ./k8s/chart-values/prometheus.yaml
else
	helm --kubeconfig ./kubeconfig.yaml install prometheus oci://ghcr.io/prometheus-community/charts/prometheus \
		--create-namespace \
		--namespace prometheus \
		--values ./k8s/chart-values/prometheus.yaml
endif

.PHONY: prometheus.update
prometheus.update:
ifneq ($(remote), 1)
	helm upgrade prometheus oci://ghcr.io/prometheus-community/charts/prometheus \
		--namespace prometheus \
		--reuse-values \
		--values ./k8s/chart-values/prometheus.yaml
else
	helm --kubeconfig ./kubeconfig.yaml upgrade prometheus oci://ghcr.io/prometheus-community/charts/prometheus \
		--namespace prometheus \
		--reuse-values \
		--values ./k8s/chart-values/prometheus.yaml
endif

.PHONY: prometheus.rm
prometheus.rm:
ifneq ($(remote), 1)
	helm uninstall prometheus --namespace prometheus --ignore-not-found
else
	helm --kubeconfig ./kubeconfig.yaml uninstall prometheus --namespace prometheus --ignore-not-found
endif

.PHONY: traffic.spread
traffic.spread:
	K6_PROMETHEUS_RW_SERVER_URL=http://$(PROMETHEUS)/api/v1/write \
		TARGET_HOSTNAMES=$(HOSTNAMES) \
		k6 run --out experimental-prometheus-rw ./traffic/spread.js

.PHONY: traffic.single
traffic.single:
	K6_PROMETHEUS_RW_SERVER_URL=http://$(PROMETHEUS)/api/v1/write \
		TARGET_HOSTNAME=$(HOSTNAME) \
		k6 run --out experimental-prometheus-rw ./traffic/single.js
