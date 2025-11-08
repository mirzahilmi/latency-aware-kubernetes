package extender

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/rs/zerolog/log"
)

func (e *Extender) HandleScore(w http.ResponseWriter, r *http.Request) {
	var req SchedulerRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	log.Info().Str("pod", req.Pod.Metadata.Name).
		Int("nodeCount", len(req.Nodes.Items)).
		Msg("[SCORE] Request received")

	// refresh prober & traffic so when new pod scheduled, it doesn’t reuse stale cache data
	e.RefreshProberData()
	e.RefreshTrafficData()

	e.mu.RLock()
	proberData := e.proberScores
	trafficNorm := e.cachedTrafficNorm
	e.mu.RUnlock()

	//scoring phase start here
	priorities := make(HostPriorityList, 0, len(req.Nodes.Items))
	var bestNode string
	var bestScore float64

	log.Info().Msg("[SCORE]Scoring mode active, using latency, CPU, and traffic")

	for _, node := range req.Nodes.Items {
		nodeName := node.Metadata.Name
		ps, ok := proberData[nodeName]
		if !ok {
			log.Debug().Msgf("[SCORE] Skipping node %s: no prober data", nodeName)
			continue
		}

		scoreNormal := ScoreNode(nodeName, proberData, trafficNorm, e.Config)
		if scoreNormal <= 0 {
			continue
		}

		priorities = append(priorities, HostPriority{Host: nodeName, Score: scoreNormal})
		if float64(scoreNormal) > bestScore {
			bestNode, bestScore = nodeName, float64(scoreNormal)
		}
		log.Debug().Msgf("[SCORE] Node %s score=%d (CPU=%.3f Lat=%.3f Traffic=%.3f)", 
			nodeName, scoreNormal, ps.CPUEwmaScore, ps.LatencyEwmaScore, trafficNorm[nodeName])
		}

		if bestNode == "" {
			log.Warn().Msg("[SCORE] No valid node found during normal scoring")
		} else {
			log.Info().Msgf("[SCORE] Selected best node: %s (score=%.3f)", bestNode, bestScore)
		}

		if bestNode != "" {
			e.ApplyPenalty(bestNode, float64(bestScore))
		} else {
			log.Warn().Msg("[SCORE] No valid node selected — skipping penalty")
		}

		log.Info().Msgf("[SCORE] Completed — best=%s (score=%.2f)", bestNode, bestScore)

	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(priorities); err != nil {
		log.Error().Err(err).Msg("[SCORE] Failed to encode response")
	}
}


func (e *Extender) ApplyPenalty(bestNode string, bestScore float64) {
	e.mu.Lock()
	defer e.mu.Unlock()

	ps, ok := e.proberScores[bestNode]
	if !ok {
		log.Warn().Msgf("[SCORE] Cannot apply penalty — node %s not found in cache", bestNode)
		return
	}

	oldCPU := ps.CPUEwmaScore
	ps.CPUEwmaScore = ApplyCPUPenalty(bestNode, oldCPU, e.Config)
	e.proberScores[bestNode] = ps

	e.lastPenalized[bestNode] = struct {
		CPU     float64
		Applied time.Time
	}{
		CPU:     ps.CPUEwmaScore,
		Applied: time.Now(),
	}

	log.Debug().Msgf("[SCORE] Penalized %s (cpu: %.2f→%.2f)", bestNode, oldCPU, ps.CPUEwmaScore)
}

