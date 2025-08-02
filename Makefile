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

.PHONY: gateway
gateway:
	@echo "# Installing Gateway API CRDs from the Standard channel."
	kubectl apply --filename https://github.com/kubernetes-sigs/gateway-api/releases/download/v1.2.1/standard-install.yaml
	@echo "# Installing Traefik RBACs."
	kubectl apply --filename https://raw.githubusercontent.com/traefik/traefik/v3.4/docs/content/reference/dynamic-configuration/kubernetes-gateway-rbac.yml
	@echo "# Installing Traefik Helm Chart"
	helm repo add traefik https://traefik.github.io/charts
	helm repo update
	helm install traefik traefik/traefik \
		--create-namespace \
		--namespace traefik-gateway-api \
		--values ./k8s/chart-values/traefik.yaml

.PHONY: stack
stack:
	@echo "# Loading hellopod image ahead..."
	docker pull ghcr.io/mirzahilmi/hellopod:0.1.0
	kind load docker-image ghcr.io/mirzahilmi/hellopod:0.1.0
	kubectl apply \
		--filename ./k8s/Namespace.yaml \
		--filename ./k8s/Deployment.yaml \
		--filename ./k8s/Service.yaml

.PHONY: stack.rm
stack.rm:
	kubectl delete \
		--filename ./k8s/Namespace.yaml \
		--filename ./k8s/Deployment.yaml \
		--filename ./k8s/Service.yaml

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
	cilium status --wait

.PHONY: metallb
metallb:
	@echo "Adding metallb into helm repo..."
	helm repo add metallb https://metallb.github.io/metallb
	@echo "Installing metallb..."
	helm install metallb metallb/metallb \
		--create-namespace \
		--namespace metallb-system \
		--values ./k8s/chart-values/metallb.yaml
	@echo "Waiting for controller & speakers to be ready..."
	kubectl --namespace metallb-system wait pod \
		--all --timeout=90s \
		--for=condition=Ready
	kubectl --namespace metallb-system wait deploy metallb-controller \
		--timeout=90s --for=condition=Available
	kubectl --namespace metallb-system wait apiservice v1beta1.metallb.io \
		--timeout=90s --for=condition=Available

.PHONY: metallb.crd
metallb.crd:
	kubectl apply \
		--filename ./k8s/IPAddressPool.yaml \
		--filename ./k8s/L2Advertisement.yaml
