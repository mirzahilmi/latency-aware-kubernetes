# append `remote=1 only for remote cluster`

GREEN=\033[0;32m
YELLOW=\033[0;33m
RESET=\033[0m

.PHONY: help
help:
	@printf "$(GREEN)> %s$(RESET)\n" "No helps today, check the Makefile 😛"

.PHONY: all
all:
	make cluster
	make namespace
	make otelcol
	make prober
	make rater
	make stack

.PHONY: bye
bye:
ifeq ($(remote), 1)
	@printf "$(YELLOW)> %s$(RESET)\n" "Truncating resource on remote cluster is not supported"
else
	make cluster.rm
	@printf "$(GREEN)> %s$(RESET)\n" "Bye bye 👋"
endif

.PHONY: cluster
cluster:
ifneq ($(remote), 1)
	@printf "$(GREEN)> %s...$(RESET)\n" "Pulling kind docker image ahead-of-time"
	docker pull kindest/node:v1.34.0
	@printf "$(GREEN)> %s...$(RESET)\n" "Creating kind cluster"
	kind create cluster --image kindest/node:v1.34.0 --config ./k8s/Cluster.yaml
	@printf "$(GREEN)> %s...$(RESET)\n" "Patching kind cluster to include metrics-server"
	kubectl apply \
		--filename https://github.com/kubernetes-sigs/metrics-server/releases/download/v0.8.0/components.yaml
	kubectl patch -n kube-system deployment metrics-server --type=json \
		-p '[{"op":"add","path":"/spec/template/spec/containers/0/args/-","value":"--kubelet-insecure-tls"}]'
else
	@printf "$(GREEN)> %s...$(RESET)\n" "Skipping cluster creation on remote cluster"
endif

.PHONY: cluster.rm
cluster.rm:
ifneq ($(remote), 1)
	kind delete cluster
else
	@printf "$(YELLOW)> %s...$(RESET)\n" "Remote cluster removal is not supported"
endif

.PHONY: namespace
namespace:
	kubectl apply --filename ./k8s/Namespace.yaml

.PHONY: stack
stack:
ifneq ($(remote), 1)
	@printf "$(GREEN)> %s...$(RESET)\n" "Pulling & loading hellopod image ahead-of-time"
	docker pull ghcr.io/mirzahilmi/hellopod:0.2.0
	kind load docker-image ghcr.io/mirzahilmi/hellopod:0.2.0
endif
	@printf "$(GREEN)> %s...$(RESET)\n" "Applying resources"
	kubectl apply --filename ./k8s/Hellopod.yaml
ifneq ($(remote), 1)
	kubectl rollout --namespace riset status deployment hellopod
	(&>/dev/null kubectl port-forward --namespace riset service/hellopod-np-svc 30000:8000 &)
endif

.PHONY: stack.rm
stack.rm:
	kubectl delete --ignore-not-found=true \
		--filename ./k8s/Hellopod.yaml

.PHONY: prober
prober:
ifneq ($(remote), 1)
	@printf "$(GREEN)> %s...$(RESET)\n" "Pulling & loading prober image ahead-of-time"
	docker pull ghcr.io/mirzahilmi/prober:0.3.5
	kind load docker-image ghcr.io/mirzahilmi/prober:0.3.5
endif
	@printf "$(GREEN)> %s...$(RESET)\n" "Applying resources"
	kubectl apply --filename ./k8s/Prober.yaml

.PHONY: prober.rm
prober.rm:
	@printf "$(GREEN)> %s...$(RESET)\n" "Deleting resources"
	kubectl delete --filename ./k8s/Prober.yaml

.PHONY: rater
rater:
ifneq ($(remote), 1)
	@printf "$(YELLOW)> %s...$(RESET)\n" "Skipping http_rater deployment on local cluster"
else
	@printf "$(GREEN)> %s...$(RESET)\n" "Applying resources"
	kubectl apply --filename ./k8s/HttpRater.yaml
endif

.PHONY: rater.rm
rater.rm:
	@printf "$(GREEN)> %s...$(RESET)\n" "Deleting resources"
	kubectl delete --filename ./k8s/HttpRater.yaml

.PHONY: otelcol
otelcol:
ifneq ($(remote), 1)
		@printf "$(YELLOW)> %s...$(RESET)\n" "Skipping OTLP Collector deployment on local cluster"
else
	kubectl apply --filename ./k8s/Secret.yaml
	helm repo add open-telemetry https://open-telemetry.github.io/opentelemetry-helm-charts
	helm install opentelemetry-collector open-telemetry/opentelemetry-collector \
		--namespace riset \
		--values ./k8s/chart-values/opentelemetry_collector.yaml
endif

.PHONY: otelcol.re
otelcol.re:
	helm upgrade opentelemetry-collector open-telemetry/opentelemetry-collector \
		--namespace riset \
		--reuse-values \
		--values ./k8s/chart-values/opentelemetry_collector.yaml

.PHONY: otelcol.rm
otelcol.rm:
	helm uninstall opentelemetry-collector --namespace riset --ignore-not-found

