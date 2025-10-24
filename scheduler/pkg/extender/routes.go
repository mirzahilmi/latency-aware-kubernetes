package extender

import (
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
	"github.com/rs/zerolog/log"
)

func NewExtender(influxService *influx.Service, bucket string) *Extender {
	e := &Extender{
		influxService: influxService,
		bucket:        bucket,
		proberScores:  make(map[string]prober.ScoreData),
		// distribution:  NewPodDistribution(1), // minimal 1 pod per node
	}
	go e.refreshMetricsLoop()
	//go e.refreshDistributionLoop()
	return e
}

func (e *Extender) refreshMetricsLoop() {
	refreshInterval := 60 * time.Second

	for {
		topNode, _, err := e.influxService.QueryTopNode(e.bucket)
		if err != nil {
			log.Warn().Err(err).Msg("[EXTENDER] Failed to query top node from InfluxDB")
			time.Sleep(10 * time.Second)
			continue
		}
		e.topNode = topNode
		log.Info().Msgf("[EXTENDER] Found top node: %s", topNode)

		// trafficMap, err := e.influxService.QueryTrafficByNode(e.bucket)
		// if err != nil {
		// 	log.Warn().Err(err).Msg("[EXTENDER] Failed to query traffic map from InfluxDB")
		// } else {
		// 	e.mu.Lock()
		// 	e.cachedTraffic = trafficMap
		// 	e.mu.Unlock()
		// 	log.Info().Msgf("[EXTENDER] Cached traffic map for %d nodes", len(trafficMap))
		// }

		scores, err := prober.FetchScoresFromNode(topNode)
		if err != nil {
			log.Warn().Err(err).Msgf("[EXTENDER] Failed to fetch prober data from %s", topNode)
			time.Sleep(refreshInterval)
			continue
		}
		log.Info().Msgf("[EXTENDER] Fetched prober data from top node %s", topNode)
		e.mu.Lock()
		for _, s := range scores {
			e.proberScores[s.Hostname] = s
		}
		e.mu.Unlock()

		time.Sleep(refreshInterval)
	}
}

func (e *Extender) RegisterRoutes(router *chi.Mux) {
	router.Post("/filter", e.HandleFilter)
	router.Post("/score", e.HandleScore)
	router.Get("/health", e.HandleHealth)
}

func (e *Extender) HandleHealth(w http.ResponseWriter, _ *http.Request) {
	if len(e.proberScores) > 0 {
		w.WriteHeader(http.StatusNoContent)
	} else {
		http.Error(w, "not ready", http.StatusServiceUnavailable)
		return
	}
	// json.NewEncoder(w).Encode(data)
}
