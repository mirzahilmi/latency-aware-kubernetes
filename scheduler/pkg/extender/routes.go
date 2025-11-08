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
		lastPenalized:     make(map[string]struct {
			CPU     float64
			Applied time.Time
   		}),
	}
	return e
}

func (e *Extender) RegisterRoutes(router *chi.Mux) {
	router.Post("/filter", e.HandleFilter)
	router.Post("/score", e.HandleScore)
	router.Post("/bind", e.HandleBind)
	router.Get("/health", e.HandleHealth)
}

// Warmup phase: extender will fetch prober data from configured top node in the beginning of scheduling phase
// so make sure that your configured top node is the most powerful one (cause scheduler indirectly would recognize top node as the best node)
func (e *Extender) Warmup() {
    log.Info().Msg("[WARMUP] Pre-fetching baseline prober metrics...")

    topNode, rate, err := e.influxService.QueryTopNode(e.bucket)
    if err != nil {
        log.Warn().Err(err).Msg("[WARMUP] Failed to query top node from InfluxDB")
    }

    fallbackNode := os.Getenv("TOPNODE_FB")
    if fallbackNode == "" {
        log.Warn().Msg("[WARMUP] No fallback node configured (TOPNODE_FB)")
    }

    var targetNode string
    if topNode == "" {
		targetNode = fallbackNode
		log.Warn().Msgf("[WARMUP] No topNode found, using fallback %s", targetNode)
	} else {
		targetNode = topNode
		log.Info().Msgf("[WARMUP] Using topNode %s (traffic=%.2f req/min)", targetNode, rate)
	}

    scores, err := prober.FetchScoresFromNode(targetNode)
    if err != nil || len(scores) == 0 {
        log.Warn().Err(err).Msgf("[WARMUP] Failed to fetch prober data from %s", targetNode)
        return
    }

    e.mu.Lock()
    for _, s := range scores {
        e.proberScores[s.Hostname] = s
    }
    e.mu.Unlock()
    log.Info().Msgf("[WARMUP] Completed, cached baseline metrics from %s (%d nodes)", targetNode, len(scores))
}

func (e *Extender) RefreshProberData() {
    topNode, topVal, err := e.influxService.QueryTopNode(e.bucket)
    if err != nil || topNode == "" {
        log.Warn().Err(err).Msg("[EXTENDER] Failed to query top node from InfluxDB")
        return
    }

    e.mu.Lock()
	e.mu.Unlock()

    log.Info().Msgf("[EXTENDER] Refreshed prober data from %s (traffic=%.2f)", topNode, topVal)

    scores, err := prober.FetchScoresFromNode(topNode)
    if err != nil {
        log.Warn().Err(err).Msgf("[EXTENDER] Failed to fetch prober data from %s", topNode)
        return
    }

    e.mu.Lock()
	defer e.mu.Unlock()

    for _, s := range scores {
		if p, ok := e.lastPenalized[s.Hostname]; ok {
			if time.Since(p.Applied) < 15*time.Second {
				s.CPUEwmaScore = p.CPU
				log.Debug().Msgf("[EXTENDER] Keeping penalized CPU for %s (recent <15s)", s.Hostname)
			} else {
				delete(e.lastPenalized, s.Hostname)
				log.Debug().Msgf("[EXTENDER] Removing expired penalty for %s", s.Hostname)
			}
		}
		e.proberScores[s.Hostname] = s
	}

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
