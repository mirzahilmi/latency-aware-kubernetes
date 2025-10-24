package extender

import (
	"encoding/json"
	"math"
	"net/http"
	"strings"

	"github.com/rs/zerolog/log"
)

// HandleScore menghitung skor node dan menerapkan penalti CPU
// hanya setelah node terbaik (winner) ditentukan.
func (e *Extender) HandleScore(w http.ResponseWriter, r *http.Request) {
	var req SchedulerRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	log.Info().
		Str("[SCORE] pod", req.Pod.Metadata.Name).
		Int("[SCORE] nodeCount", len(req.Nodes.Items)).
		Msg("[SCORE] Score request received")

	e.mu.RLock()
	proberData := e.proberScores
	trafficMap := e.cachedTraffic
	e.mu.RUnlock()

	if len(proberData) == 0 {
		log.Warn().Msg("[SCORE] no prober data available — returning neutral scores")
	}

	priorities := make(HostPriorityList, 0, len(req.Nodes.Items))

	var bestNode string
	var bestScore float64

	// 🧮 Phase 1: hitung skor untuk semua node
	for _, node := range req.Nodes.Items {
		nodeName := node.Metadata.Name

		// base score: kombinasi latency + CPU
		baseScore := ScoreNode(nodeName, proberData, trafficMap)
		if baseScore == 0 {
			log.Warn().Msgf("[SCORE] base score is zero for %s", nodeName)
		}

		normalized := int(math.Round(baseScore / 10000))
		if normalized > 100 {
			normalized = 100
		}
		if normalized < 0 {
			normalized = 0
		}

		priorities = append(priorities, HostPriority{
			Host:  nodeName,
			Score: normalized,
		})

		if baseScore > bestScore {
			bestScore = baseScore
			bestNode = nodeName
		}
	}

	// 🚀 Phase 2: setelah node terbaik ditentukan → beri penalti CPU
	if bestNode != "" {
		e.mu.Lock()
		nodeScore := e.proberScores[bestNode]
		oldCPU := nodeScore.CPUEwmaScore

		// update distribusi count (1 pod dijadwalin ke node ini)
		// e.distribution.UpdateCount(bestNode, 1)

		// kasih penalti CPU sesuai tipe node
		if strings.Contains(bestNode, "vm") {
			nodeScore.CPUEwmaScore -= 0.025 // 2 core (VM)
		} else {
			nodeScore.CPUEwmaScore -= 0.0125 // 4 core (Raspberry)
		}
		if nodeScore.CPUEwmaScore < 0 {
			nodeScore.CPUEwmaScore = 0
		}

		e.proberScores[bestNode] = nodeScore
		e.mu.Unlock()

		log.Info().
			Str("bestNode", bestNode).
			Float64("bestScore", bestScore).
			Float64("cpuBefore", oldCPU).
			Float64("cpuAfter", nodeScore.CPUEwmaScore).
			Msg("[SCORE] Winner node penalized and CPU cache updated")
	} else {
		log.Warn().Msg("[SCORE] No best node selected — no penalty applied")
	}

	log.Info().
		Int("[SCORE SUM] scoredNodes", len(priorities)).
		Msg("[SCORE PHASE COMPLETED]")

	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(priorities); err != nil {
		log.Error().Err(err).Msg("[SCORE] Failed to encode score response")
	}
}
