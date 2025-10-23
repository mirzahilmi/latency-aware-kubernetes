package main

import (
	"context"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/descheduler"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/logging"
	"github.com/rs/zerolog/log"
	"k8s.io/client-go/kubernetes"
	"k8s.io/client-go/rest"
)

func main() {
	// === Logging ===
	logging.Configure()
	log.Info().Msg("[DESCHEDULER] Starting Descheduler")

	// === InfluxDB Service ===
	influxSvc := influx.NewService()
	if influxSvc == nil {
		log.Fatal().Msg("[DESCHEDULER] Failed to initialize InfluxDB client")
	}

	bucket := os.Getenv("INFLUX_BUCKET")
	if bucket == "" {
		log.Fatal().Msg("[DESCHEDULER] Missing INFLUX_BUCKET environment variable")
	}

	// === Kubernetes Client ===
	config, err := rest.InClusterConfig()
	if err != nil {
		log.Fatal().Err(err).Msg("[DESCHEDULER] Cannot load in-cluster kubeconfig")
	}

	clientset, err := kubernetes.NewForConfig(config)
	if err != nil {
		log.Fatal().Err(err).Msg("[DESCHEDULER] Cannot create Kubernetes clientset")
	}

	// === Descheduler Initialization ===
	ds := descheduler.NewDescheduler(clientset, influxSvc, bucket)

	// === Context & Graceful Shutdown ===
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()

	interval := 1 * time.Minute
	log.Info().Msgf("[DESCHEDULER] Loop started (interval: %v)", interval)

	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			log.Info().Msg("[DESCHEDULER] Stopped gracefully")
			return
		case <-ticker.C:
			log.Info().Msg("[DESCHEDULER] Evaluating cluster state...")
			ds.Evaluate(ctx)
		}
	}
}
