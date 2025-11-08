package main

import (
	"context"
	"os"
	"os/signal"
	"syscall"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/descheduler"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/extender"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/rs/zerolog/log"
	"k8s.io/client-go/kubernetes"
	"k8s.io/client-go/rest"
)

func main() {
	log.Info().Msg("[DESCHEDULER] Starting Adaptive Descheduler Controller")

	// === Context & graceful shutdown ===
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()

	// === In-cluster Kubernetes config ===
	config, err := rest.InClusterConfig()
	if err != nil {
		log.Fatal().Err(err).Msg("[DESCHEDULER] Failed to load in-cluster config")
	}
	clientset, err := kubernetes.NewForConfig(config)
	if err != nil {
		log.Fatal().Err(err).Msg("[DESCHEDULER] Failed to create clientset")
	}

	// === InfluxDB client ===
	influxSvc := influx.NewService()
	if influxSvc == nil {
		log.Fatal().Msg("[DESCHEDULER] Failed to initialize InfluxDB client")
	}

	// === Base initialization ===
	namespace := os.Getenv("POD_NAMESPACE")
	cfg := extender.LoadScoringConfig()

	// === Initialize descheduler instance ===
	ds := descheduler.NewAdaptiveDescheduler(clientset, influxSvc, influxSvc.GetBucket(), namespace, cfg)

	// === Watch CRD: LatencyDeschedulerPolicy ===
	log.Info().Msg("[DESCHEDULER] Watching LatencyDeschedulerPolicy CRD...")
	if err := ds.WatchLatencyPolicy(ctx); err != nil {
		log.Fatal().Err(err).Msg("[DESCHEDULER] CRD watch failed — cannot proceed")
	}
}
