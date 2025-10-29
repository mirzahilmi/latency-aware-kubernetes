package main

import (
	"net/http"
	"os"

	"github.com/go-chi/chi/v5"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/extender"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	// "github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/logging"
	"github.com/rs/zerolog/log"
)

func main() {
	// === Logging ===
	// logging.Configure()
	log.Info().Msg("[EXTENDER] Starting Latency-Aware Scheduler Extender")

	// === InfluxDB Service ===
	influxSvc := influx.NewService()
	if influxSvc == nil {
		log.Fatal().Msg("[EXTENDER] Failed to initialize InfluxDB client")
	}

	bucket := os.Getenv("INFLUX_BUCKET")
	if bucket == "" {
		log.Fatal().Msg("[EXTENDER] Missing INFLUX_BUCKET environment variable")
	}

	// === Initialize Extender ===
	ext := extender.NewExtender(influxSvc, bucket)

	// === HTTP Router ===
	router := chi.NewMux()
	ext.RegisterRoutes(router) // register /filter, /score, /health

	port := os.Getenv("PORT_EXTENDER")
	if port == "" {
		port = "3001"
	}

	addr := ":" + port
	log.Info().Msgf("[EXTENDER] Scheduler extender running on %s", addr)

	// === Start HTTP Server ===
	if err := http.ListenAndServe(addr, router); err != nil {
		log.Fatal().Err(err).Msg("[EXTENDER] Failed to start extender HTTP server")
	}
}
