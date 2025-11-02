package extender

import (
	"net/http"
	"os"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
	"github.com/rs/zerolog/log"
)

// NewExtender initializes the scheduler extender
func NewExtender(influxService *influx.Service, bucket string) *Extender {
	e := &Extender{
		influxService: influxService,
		bucket:        bucket,
		proberScores:  make(map[string]prober.ScoreData),
		cachedTraffic: make(map[string]float64),
		cachedTrafficNorm: make(map[string]float64),
		IsColdStart: true,
	}
	go e.Warmup()
	return e
}

func (e *Extender) RegisterRoutes(router *chi.Mux) {
	router.Post("/filter", e.HandleFilter)
	router.Post("/score", e.HandleScore)
	router.Post("/bind", e.HandleBind)
	router.Get("/health", e.HandleHealth)
}

// Warmup: only check connectivity (non-blocking), not waiting for traffic.
func (e *Extender) Warmup() {
	log.Info().Msg("[WARMUP] Cold start: verifying InfluxDB & Prober connectivity...")

	for {
		// Check Influx connectivity
		if _, _, err := e.influxService.QueryTopNode(e.bucket); err != nil {
			log.Warn().Err(err).Msg("[WARMUP] Waiting for InfluxDB connection...")
			time.Sleep(3 * time.Second)
			continue
		}

		// Fetch from configured NODE or fallback topNode
		nodeName := os.Getenv("TOPNODE_FB")
		if nodeName == "" {
			topNode, _, err := e.influxService.QueryTopNode(e.bucket)
			if err == nil && topNode != "" {
				nodeName = topNode
			}
		}

		if nodeName == "" {
			log.Warn().Msg("[WARMUP] No valid node to fetch prober data, retrying...")
			time.Sleep(3 * time.Second)
			continue
		}

		scores, err := prober.FetchScoresFromNode(nodeName)
		if err != nil || len(scores) == 0 {
			log.Warn().Err(err).Msgf("[COLD-START] Waiting for prober metrics from %s...", nodeName)
			time.Sleep(3 * time.Second)
			continue
		}

		// Cache minimal prober data
		e.mu.Lock()
		for _, s := range scores {
			e.proberScores[s.Hostname] = s
		}
		e.IsColdStart = false
		e.mu.Unlock()

		log.Info().Msgf("[COLD-START] Warmup complete — metrics fetched from %s", nodeName)
		break
	}
}

func (e *Extender) RefreshProberData() {
	topNode, topVal, err := e.influxService.QueryTopNode(e.bucket)
	if err != nil || topNode == "" {
		log.Warn().Err(err).Msg("[EXTENDER] Failed to query top node from InfluxDB")
	}

	e.mu.Lock()
	if e.IsColdStart && topNode != "" && topVal >= e.Config.WarmupThreshold {
		e.IsColdStart = false
		log.Info().Str("mode", "NORMAL").
			Msgf("[COLD→NORMAL] Transition complete — req=%.2f from %s", topVal, topNode)

	}
	e.mu.Unlock()

	fallbackNode := os.Getenv("TOPNODE_FB")
	if topNode == "" || topVal < e.Config.WarmupThreshold {
		log.Warn().Msgf("[EXTENDER] topNode=%s traffic=%.2f (<%.2f) → using fallback node %s", topNode, topVal, e.Config.WarmupThreshold, fallbackNode)
		topNode = fallbackNode
	}

	log.Info().Msgf("[EXTENDER] Refreshed top node for prober: %s", topNode)

	scores, err := prober.FetchScoresFromNode(topNode)
	if err != nil {
		log.Warn().Err(err).Msgf("[EXTENDER] Failed to fetch prober data from %s", topNode)
		return
	}

	e.mu.Lock()
	for _, s := range scores {
		e.proberScores[s.Hostname] = s
	}
	e.mu.Unlock()

	log.Info().Msgf("[EXTENDER] Updated prober data from %s (%d nodes)", topNode, len(scores))
}


// RefreshTrafficData updates cached traffic and normalized traffic maps.
func (e *Extender) RefreshTrafficData() {
	trafficMap, err := e.influxService.QueryTrafficByNode(e.bucket)
	if err != nil {
		log.Warn().Err(err).Msg("[EXTENDER] Failed to query traffic map")
		return
	}

	trafficNormMap, err := e.influxService.NormalizedTraffic(e.bucket)
	if err != nil {
		log.Warn().Err(err).Msg("[EXTENDER] Failed to query normalized traffic")
		return
	}

	e.mu.Lock()
	e.cachedTraffic = trafficMap
	e.cachedTrafficNorm = trafficNormMap
	e.mu.Unlock()

	log.Info().Msgf("[EXTENDER] Updated traffic map (%d entries) + normalized traffic", len(trafficMap))
}

// HandleHealth is a simple liveness endpoint
func (e *Extender) HandleHealth(w http.ResponseWriter, _ *http.Request) {
	e.mu.RLock()
	hasProberData := len(e.proberScores) > 0
	e.mu.RUnlock()

	if hasProberData {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("ok"))
		return
	}

	// Still healthy, just not fully initialized
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write([]byte("warming up — waiting for prober metrics"))
	log.Debug().Msg("[HEALTH] Warming up (still healthy)")
}
