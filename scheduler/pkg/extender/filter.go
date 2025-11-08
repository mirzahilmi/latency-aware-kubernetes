package extender

import (
	"encoding/json"
	"net/http"

	"github.com/rs/zerolog/log"
)

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


	e.RefreshProberData()

	e.mu.RLock()
	proberData := e.proberScores
	e.mu.RUnlock()

	// filter result
	result := ExtenderFilterResult{
		Nodes:       &NodeList{Items: make([]Node, 0)},
		FailedNodes: make(map[string]string),
	}

	if len(proberData) == 0 {
		log.Warn().Msg("[FILTER] No prober data available - passing all nodes")
		result.Nodes = &req.Nodes
		
		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(result); err != nil {
			log.Error().Err(err).Msg("[FILTER] Failed to encode response")
		}
		return
	}

	for _, node := range req.Nodes.Items {
		nodeName := node.Metadata.Name
		score, ok := e.proberScores[nodeName]
		if !ok {
			result.FailedNodes[nodeName] = "no prober data"
			log.Debug().Msgf("[FILTER] %s REJECTED: no prober data", nodeName)
			continue
		}

		if score.LatencyEwmaScore < e.Config.LatencyThreshold {
			result.FailedNodes[nodeName] = "latency below threshold"
			log.Debug().Msgf("[FILTER] %s REJECTED: latency=%.3f < %.2f", 
				nodeName, score.LatencyEwmaScore, e.Config.LatencyThreshold)
			continue
		}

		if score.CPUEwmaScore < e.Config.cpuThreshold {
			result.FailedNodes[nodeName] = "cpu below threshold"
			log.Debug().Msgf("[FILTER] %s REJECTED: cpu=%.3f < %.2f", 
				nodeName, score.CPUEwmaScore, e.Config.cpuThreshold)
			continue
		}

		// Node passed all threshold
		result.Nodes.Items = append(result.Nodes.Items, node)
		log.Debug().Msgf("[FILTER] %s PASSED (cpu=%.3f latency=%.3f)", 
			nodeName, score.CPUEwmaScore, score.LatencyEwmaScore)
	}

	// logging summary
	log.Info().
		Int("[FILTER SUM] passedNodes", len(result.Nodes.Items)).
		Int("[FILTER SUM] failedNodes", len(result.FailedNodes)).
		Msg("[FILTER PHASE COMPLETED]")

	if len(result.Nodes.Items) == 0 {
		log.Warn().Msg("[FILTER] All nodes filtered out! Scheduler may fail to place pod")
		for nodeName, reason := range result.FailedNodes {
			log.Warn().Msgf("[FILTER]   • %s: %s", nodeName, reason)
		}
	}

	// return response
	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(result); err != nil {
		log.Error().Err(err).Msg("[FILTER] Failed to encode filter response")
	}
}
