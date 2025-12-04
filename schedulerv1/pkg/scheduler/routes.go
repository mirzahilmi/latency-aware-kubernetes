package scheduler

import (
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"
	influxSvc "github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
	"github.com/rs/zerolog/log"
	"k8s.io/client-go/kubernetes"
)

func NewExtender(influx *influxSvc.Service, bucket string, kube kubernetes.Interface, cfg ScoringConfig) *Extender {
	return &Extender{
		influxSvc: influx,
		bucket: bucket,
		kubeClient: kube,
		cfg: cfg,
		proberScores: make(map[string]prober.ScoreData),
		traffic: make(map[string]float64),
		trafficNorm: make(map[string]float64),
		lastPenalized: make(map[string]PenaltyEntry),
	}
}

func (e *Extender) RegisterRoutes(router *chi.Mux) {
	router.Post("/filter", e.Filter)
	router.Post("/prioritize", e.Prioritize)
	router.Post("/bind", e.Bind)
	router.Get("/health", e.Health)
}

func (e *Extender) refreshProberData() {
    topNode, topVal, err := e.influxSvc.QueryTopNode(e.bucket)
    if err != nil || topNode == "" {
        log.Warn().Err(err).Msg("[EXTENDER] Failed to query top node from InfluxDB")
        return
    }
    log.Info().Msgf("[EXTENDER] Refreshed prober data from %s (traffic=%.2f)", topNode, topVal)

    scores, err := prober.FetchScoresFromNode(topNode)
    if err != nil {
        log.Warn().Err(err).Msgf("[EXTENDER] Failed to fetch prober data from %s", topNode)
        return
    }

    e.mu.Lock()
	defer e.mu.Unlock()

	
	//add this logic for cpu and memory penalty score persistence
    for _, s := range scores {
		if last, ok := e.lastPenalized[s.Hostname]; ok {
			// keep previous penalized scores within ttl value
			if time.Since(last.Applied) < time.Duration(e.cfg.penaltyTtl)*time.Second { 
				s.CPUEwmaScore = last.CPU
				s.MemoryEwmaScore = last.Mem
				log.Debug().Msgf("[EXTENDER] Keeping penalized CPU and Memory for %s (recent <%ds)", s.Hostname, e.cfg.penaltyTtl)
			} else {
				delete(e.lastPenalized, s.Hostname)
				log.Debug().Msgf("[EXTENDER] Removing expired penalty for %s", s.Hostname)
			}
		}
		e.proberScores[s.Hostname] = s
	}
    log.Info().Msgf("[EXTENDER] Updated prober data from %s (%d nodes)", topNode, len(scores))
}

// updates cached traffic and normalized traffic maps
func (e *Extender) refreshTrafficData() {
	tmap, err := e.influxSvc.QueryTrafficByNode(e.bucket)
	if err != nil {
		log.Warn().Err(err).Msg("[EXTENDER] Failed to query traffic map")
		return
	}

	nmap, err := e.influxSvc.NormalizedTraffic(e.bucket)
	if err != nil {
		log.Warn().Err(err).Msg("[EXTENDER] Failed to query normalized traffic")
		return
	}

	e.mu.Lock()
	e.traffic = tmap
	e.trafficNorm = nmap
	e.mu.Unlock()

	log.Info().Msgf("[EXTENDER] Updated traffic map (%d entries) + normalized traffic", len(tmap))
}

func (e *Extender) Health(w http.ResponseWriter, _ *http.Request) {
	e.mu.RLock()
	ready := len(e.proberScores) > 0
	e.mu.RUnlock()

	w.WriteHeader(http.StatusOK)
	if ready {
	_, _ = w.Write([]byte("ok"))
	} else {
	_, _ = w.Write([]byte("warming up"))
	}
}
