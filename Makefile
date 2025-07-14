.PHONY: all
all:
	make cluster
	make gateway
	make stack

.PHONY: all.rm # actually just an alias to cluster.rm
all.rm:
	make cluster.rm

.PHONY: cluster
cluster:
	kind create cluster --config ./k8s/Cluster.yaml

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
		--values ./k8s/traefik-values.yaml

.PHONY: stack
stack:
	kubectl apply \
		--filename ./k8s/Namespace.yaml \
		--filename ./k8s/Deployment.yaml \
		--filename ./k8s/Gateway.yaml \
		--filename ./k8s/HTTPRoute.yaml \
		--filename ./k8s/Service.yaml

.PHONY: stack.rm
stack.rm:
	kubectl delete \
		--filename ./k8s/Namespace.yaml \
		--filename ./k8s/Deployment.yaml \
		--filename ./k8s/Gateway.yaml \
		--filename ./k8s/HTTPRoute.yaml \
		--filename ./k8s/Service.yaml
