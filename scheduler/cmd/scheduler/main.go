package main

import (
	"net/http"
	"os"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/extender"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/rs/zerolog/log"
)

func main() {
	log.Info().Msg("[EXTENDER] Starting Latency-Aware Scheduler Extender")

	// === InfluxDB Service ===
	influxSvc := influx.NewService()
	if influxSvc == nil {
		log.Fatal().Msg("[EXTENDER] Failed to initialize InfluxDB client")
	}

	// === Load config & create Extender ===
	cfg := extender.LoadScoringConfig()
	ext := extender.NewExtender(influxSvc, influxSvc.GetBucket())
	ext.Config = cfg

	// === HTTP Router ===
	router := chi.NewRouter()
	router.Use(middleware.Logger)
	router.Use(middleware.Recoverer)
	ext.RegisterRoutes(router) // includes /filter, /score, /bind, /health

	// === Start HTTP server ===
	port := os.Getenv("PORT_EXTENDER")
	addr := ":" + port

	go func() {
		log.Info().Msgf("[EXTENDER] HTTP server listening on %s", addr)
		if err := http.ListenAndServe(addr, router); err != nil {
			log.Fatal().Err(err).Msg("[EXTENDER] Failed to start HTTP server")
		}
	}()

	// === Passive forever (warmup goroutine already running) ===
	select {}
}
