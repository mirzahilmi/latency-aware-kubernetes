package main

import (
	"context"
	"os"
	"os/signal"
	"syscall"

	"github.com/rs/zerolog/log"
	"k8s.io/client-go/kubernetes"
	"k8s.io/client-go/rest"
	metricsclient "k8s.io/metrics/pkg/client/clientset/versioned"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/descheduler"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/scheduler"
)

func main() {
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()

	// load kube configs and influx service
	kubeConfig, err := rest.InClusterConfig()
	if err != nil {
		log.Fatal().Err(err).Msg("[DESCHEDULER] Failed to load kube in-cluster config")
	}
	kubeClient, err := kubernetes.NewForConfig(kubeConfig)
    if err != nil {
        log.Fatal().Err(err).Msg("[DESCHEDULER] Failed to create kube clientset")
    }
	metricsClient, err := metricsclient.NewForConfig(kubeConfig)
	if err != nil {
		log.Fatal().Err(err).Msg("[DESCHEDULER] Failed to create metrics client")
	}
	influxSvc := influx.NewService()
	if influxSvc == nil {
		log.Fatal().Msg("[DESCHEDULER] Failed to initialize InfluxDB client")
	}

	// load env configs
	scoringCfg := scheduler.LoadScoringConfig()
	deschedCfg := descheduler.LoadDeschedulerConfig()
	ns := os.Getenv("POD_NAMESPACE")

	// build adaptive descheduler & controller
	ad := descheduler.NewAdaptiveDescheduler(
		kubeClient,
		metricsClient,
		influxSvc,
		influxSvc.GetBucket(),
		ns,
		scoringCfg,
		deschedCfg,
	)

	controller := descheduler.NewController(ad)
	
	// run descheduler controller
	go controller.Run(ctx)

	log.Info().Msg("[DESCHEDULER] Shutting down complete")
}
