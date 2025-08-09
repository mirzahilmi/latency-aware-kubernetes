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
	kind create cluster --config ./k8s/Cluster.yaml
	@echo "# Loading netshoot image for network debugging..."
	docker pull bretfisher/netshoot:latest
	kind load docker-image bretfisher/netshoot:latest

.PHONY: cluster.rm
cluster.rm:
	kind delete cluster

.PHONY: stack
stack:
	@echo "# Loading hellopod image ahead..."
	docker pull ghcr.io/mirzahilmi/hellopod:0.1.1
	kind load docker-image ghcr.io/mirzahilmi/hellopod:0.1.1
	kubectl apply \
		--filename ./k8s/Namespace.yaml \
		--filename ./k8s/Hellopod.yaml \
		--filename ./k8s/Cilium.yaml

.PHONY: stack.rm
stack.rm:
	kubectl delete \
		--filename ./k8s/Namespace.yaml \
		--filename ./k8s/Hellopod.yaml \
		--filename ./k8s/Cilium.yaml

.PHONY: cilium
cilium:
	@echo "# Adding cilium into helm repo..."
	helm repo add cilium https://helm.cilium.io/
	@echo "# Pull cilium image ahead & load into kind cluster"
	docker pull quay.io/cilium/cilium:v1.17.6
	kind load docker-image quay.io/cilium/cilium:v1.17.6
	@echo "# Installing cilium..."
	helm install cilium cilium/cilium --version 1.17.6 \
		--namespace kube-system \
		--values ./k8s/chart-values/cilium.yaml
	@echo "# Waiting for cilium to be ready"
	cilium status --wait --wait-duration 10m15s
	@echo "# Pull prometheus and grafana image ahead"
	docker pull prom/prometheus:v2.42.0
	docker pull docker.io/grafana/grafana:9.3.6
	kind load docker-image prom/prometheus:v2.42.0
	kind load docker-image docker.io/grafana/grafana:9.3.6
	@echo "# Installing prometheus & grafana for observability"
	kubectl apply --filename https://raw.githubusercontent.com/cilium/cilium/1.17.6/examples/kubernetes/addons/prometheus/monitoring-example.yaml

.PHONY: cilium.update
cilium.update:
	helm upgrade cilium cilium/cilium \
		--namespace kube-system \
		--reuse-values \
		--values ./k8s/chart-values/cilium.yaml

.PHONY: load
load:
	HELLOPOD_HOSTNAME=172.18.0.3 k6 run ./traffic/load_test.js
