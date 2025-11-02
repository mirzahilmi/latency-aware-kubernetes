package extender

import (
	"encoding/json"
	"net/http"

	"github.com/rs/zerolog/log"
)

func (e *Extender) HandleScore(w http.ResponseWriter, r *http.Request) {
	var req SchedulerRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	log.Info().
		Str("pod", req.Pod.Metadata.Name).
		Int("nodeCount", len(req.Nodes.Items)).
		Msg("[SCORE] Request received")

	// refresh prober & traffic
	e.RefreshProberData()
	e.RefreshTrafficData()

	e.mu.RLock()
	proberData := e.proberScores
	trafficNormMap := e.cachedTrafficNorm
	isColdStart := e.IsColdStart
	e.mu.RUnlock()

	priorities := make(HostPriorityList, 0, len(req.Nodes.Items))
	var bestNode string
	var bestScore int64

	if isColdStart {
		log.Warn().Msg("[COLD] Cold-start mode active, using latency & CPU only (traffic ignored)")

		for _, node := range req.Nodes.Items {
			nodeName := node.Metadata.Name
			ps, ok := proberData[nodeName]
			if !ok {
				continue
			}

			penalizedCPU := ApplyCPUPenalty(nodeName, ps.CPUEwmaScore)
			rawScore := 0.6*ps.LatencyEwmaScore + 0.4*penalizedCPU // hardcoded score for cold-start
			scoreInt := clampScore(rawScore * e.Config.ScaleFactor)

			priorities = append(priorities, HostPriority{Host: nodeName, Score: scoreInt})
			if scoreInt > bestScore {
				bestScore = scoreInt
				bestNode = nodeName
			}

			log.Info().Msgf("   • %s → lat=%.3f cpu=%.3f score=%d",
				nodeName, ps.LatencyEwmaScore, ps.CPUEwmaScore, scoreInt)
		}

		if bestNode == "" {
			log.Warn().Msg("[COLD] No valid node found during cold-start scoring")
		} else {
			log.Info().Msgf("[COLD] Selected best node: %s (score=%d)", bestNode, bestScore)
		}

	} else {
		log.Info().Msg("[NORMAL] Scoring mode active — using latency, CPU, and traffic")

		for _, node := range req.Nodes.Items {
			nodeName := node.Metadata.Name
			score := ScoreNode(nodeName, proberData, trafficNormMap, e.Config)
			if score <= 0 {
				continue
			}

			priorities = append(priorities, HostPriority{Host: nodeName, Score: score})
			if score > bestScore {
				bestScore = score
				bestNode = nodeName
			}

			log.Info().Msgf("   • %s → score=%d", nodeName, score)
		}

		if bestNode == "" {
			log.Warn().Msg("[NORMAL] No valid node found during normal scoring")
		} else {
			log.Info().Msgf("[NORMAL] Selected best node: %s (score=%d)", bestNode, bestScore)
		}
	}

	if bestNode != "" {
		e.ApplyPenalty(bestNode, float64(bestScore), isColdStart)
	} else {
		log.Warn().Msg("[SCORE] No valid node selected — skipping penalty")
	}

	modeStr := map[bool]string{true: "COLD", false: "NORMAL"}[isColdStart]
	log.Info().Msgf("[SCORE] Completed (mode=%s) — best=%s (score=%d)",
		modeStr, bestNode, bestScore)

	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(priorities); err != nil {
		log.Error().Err(err).Msg("[SCORE] Failed to encode response")
	}
}

// ApplyPenalty — reduce CPU score of the winning node slightly to prevent immediate reuse
func (e *Extender) ApplyPenalty(bestNode string, bestScore float64, isCold bool) {
	e.mu.Lock()
	defer e.mu.Unlock()

	ns := e.proberScores[bestNode]
	oldCPU := ns.CPUEwmaScore
	ns.CPUEwmaScore = ApplyCPUPenalty(bestNode, ns.CPUEwmaScore)
	e.proberScores[bestNode] = ns

	mode := map[bool]string{true: "COLD", false: "NORMAL"}[isCold]
	log.Info().
		Str("mode", mode).
		Str("bestNode", bestNode).
		Int("bestScore", int(bestScore)).
		Float64("cpuBefore", oldCPU).
		Float64("cpuAfter", ns.CPUEwmaScore).
		Msg("[SCORE] Winner selected and CPU penalized")
}
