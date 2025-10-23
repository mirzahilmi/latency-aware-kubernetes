package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"os"

	"github.com/go-chi/chi/v5"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/config"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/logging"
	"github.com/rs/zerolog/log"
)

func main() {
	logging.Configure()

	configPath := os.Getenv("CONFIG_PATH")
	if configPath == "" {
		log.Fatal().Msg("scheduler: missing CONFIG_PATH")
	}
	configBytes, err := os.ReadFile(configPath)
	if err != nil {
		log.Fatal().Err(err).Msg(fmt.Sprintf("scheduler: cannot read file %s", configPath))
	}
	var config config.Config
	if err := json.NewDecoder(bytes.NewBuffer(configBytes)).Decode(&config); err != nil {
		log.Fatal().Err(err).Msg("scheduler: failed to parse config raw bytes to struct")
	}

	router := chi.NewMux()
	router.Get("/health", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	})
}
