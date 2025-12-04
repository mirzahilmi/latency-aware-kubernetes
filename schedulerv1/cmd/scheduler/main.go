package main

import (
	"net/http"
	"os"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/rs/zerolog/log"

	"k8s.io/client-go/kubernetes"
	"k8s.io/client-go/rest"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/scheduler"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	
)

func main() {
	// kube client
	kubeConfig, err := rest.InClusterConfig()
	if err != nil {
		log.Fatal().Err(err).Msg("[DESCHEDULER] Failed to load kube in-cluster config")
	}
	kubeClient, err := kubernetes.NewForConfig(kubeConfig)
	if err != nil {
		log.Fatal().Err(err).Msg("Failed to create k8s clientset")
	}

	// influx client
	influxSvc := influx.NewService()
	if influxSvc == nil {
		log.Fatal().Msg("[EXTENDER] Failed to initialize InfluxDB client")
	}

	// load config + create extender
	cfg := scheduler.LoadScoringConfig()
	ext := scheduler.NewExtender(influxSvc, influxSvc.GetBucket(), kubeClient, cfg)

	// http router here
	router := chi.NewRouter()
	router.Use(middleware.Logger)
	router.Use(middleware.Recoverer)

	ext.RegisterRoutes(router) 

	// start http server
	port := os.Getenv("PORT_EXTENDER")
	addr := ":" + port

	log.Info().Msgf("[EXTENDER] HTTP server listening on %s", addr)
	if err := http.ListenAndServe(addr, router); err != nil {
		log.Fatal().Err(err).Msg("[EXTENDER] Failed to start HTTP server")
	}
}
