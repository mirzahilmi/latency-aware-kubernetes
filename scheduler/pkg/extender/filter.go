package extender

import (
	"encoding/json"
	"net/http"

	"github.com/rs/zerolog/log"
)

// HandleFilter adalah endpoint /filter yang memfilter nodes berdasarkan threshold
// Input: SchedulerRequest (pod + nodes)
// Output: ExtenderFilterResult (filtered nodes + failed nodes)
func (e *Extender) HandleFilter(w http.ResponseWriter, r *http.Request) {
	var req SchedulerRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	log.Info().
		Str("[FILTER] pod", req.Pod.Metadata.Name).
		Int("[FILTER] nodeCount", len(req.Nodes.Items)).
		Msg("[FILTER] Filter request received")

	// hasil filter
	result := ExtenderFilterResult{
		Nodes:       &NodeList{Items: make([]Node, 0)},
		FailedNodes: make(map[string]string),
	}

	e.mu.RLock()
	defer e.mu.RUnlock()

	for _, node := range req.Nodes.Items {
		nodeName := node.Metadata.Name
		score, ok := e.proberScores[nodeName]
		if !ok {
			result.FailedNodes[nodeName] = "no prober data"
			continue
		}

		// 🧠 Latency threshold (misal SLA 400 ms → 0.25 di skala 0..1)
		if score.LatencyEwmaScore < 0.25 {
			result.FailedNodes[nodeName] = "latency below threshold"
			log.Debug().Msgf("[FILTER] %s failed: latency too low (%.3f)", nodeName, score.LatencyEwmaScore)
			continue
		}

		// ⚙️ CPU threshold (misal minimal 0.8)
		if score.CPUEwmaScore < 0.8 {
			result.FailedNodes[nodeName] = "cpu below threshold"
			log.Debug().Msgf("[FILTER] %s failed: CPU score %.3f < 0.8", nodeName, score.CPUEwmaScore)
			continue
		}

		// ✅ Node lolos semua syarat
		result.Nodes.Items = append(result.Nodes.Items, node)
		log.Debug().Msgf("[FILTER] %s passed (CPU: %.3f, Latency: %.3f)", nodeName, score.CPUEwmaScore, score.LatencyEwmaScore)
	}

	// logging summary
	log.Info().
		Int("[FILTER SUM] passedNodes", len(result.Nodes.Items)).
		Int("[FILTER SUM] failedNodes", len(result.FailedNodes)).
		Msg("[FILTER PHASE COMPLETED]")

	// return response
	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(result); err != nil {
		log.Error().Err(err).Msg("[FILTER] Failed to encode filter response")
	}
}
