package scheduler

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/rs/zerolog/log"
	extenderv1 "k8s.io/kube-scheduler/extender/v1"
)

// handles /prioritize request from the scheduler (pkg/scheduler/routes.go)
func (e *Extender) Prioritize(w http.ResponseWriter, r *http.Request) {
	// ctx := r.Context()
	
	var args extenderv1.ExtenderArgs
	if err := json.NewDecoder(r.Body).Decode(&args); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	log.Info().Str("pod", args.Pod.Name).
		Int("nodeCount", len(args.Nodes.Items)).
		Msg("[PRIORITIZE] Request received")

	// refresh prober & traffic so when new pod scheduled, it doesn’t reuse stale cache data
	e.refreshProberData()
	e.refreshTrafficData()

	e.mu.RLock()
	pmap := e.proberScores
	tmap := e.trafficNorm
	e.mu.RUnlock()

	//scoring phase start here
	priorities := make(extenderv1.HostPriorityList, 0, len(args.Nodes.Items))

	var bestNode string
	var bestScore int64 = -1

	for _, node := range args.Nodes.Items {
		nodes := node.Name

		score := ScoreNode(nodes, pmap, tmap, e.cfg)

		priorities = append(priorities, extenderv1.HostPriority{
			Host:  nodes,
			Score: score,
		})

		if score > bestScore {
			bestScore = score
			bestNode = nodes
		}
		log.Debug().Str("node", nodes).Int64("score", score).Msg("[PRIORITIZE] Node scored")
	}

	//penalty logic starts here
	if bestNode != "" {
		log.Info().Str("node", bestNode).Int64("score", bestScore).Msg("[PRIORITIZE] Best node selected")
		e.ApplyPenalty(bestNode, float64(bestScore))
	} else {
		log.Warn().Msg("[PRIORITIZE] No valid node selected, skipping penalty")
	}

	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(priorities); err != nil {
		log.Error().Err(err).Msg("[PRIORITIZE] Failed to encode response")
	}
}


func (e *Extender) ApplyPenalty(bestNode string, bestScore float64) {
	e.mu.Lock()
	defer e.mu.Unlock()

	pmap, ok := e.proberScores[bestNode]
	if !ok {
		log.Warn().Msgf("[SCORE] Cannot apply penalty, node %s not found in cache", bestNode)
		return
	}

	oldCPU := pmap.CPUEwmaScore
	oldMem := pmap.MemoryEwmaScore
	pmap.CPUEwmaScore = ApplyCPUPenalty(bestNode, oldCPU, e.cfg)
	pmap.MemoryEwmaScore = ApplyMemPenalty(bestNode, oldMem, e.cfg)

	e.proberScores[bestNode] = pmap

	e.lastPenalized[bestNode] = struct {
		CPU     float64
		Mem 	float64
		Applied time.Time
	}{
		CPU:     pmap.CPUEwmaScore,
		Mem:	 pmap.MemoryEwmaScore,
		Applied: time.Now(),
	}

	log.Debug().Msgf("[PRIORITIZE] Penalized %s (cpu: %.2f → %.2f, memory:%.2f → %.2f)", 
		bestNode, oldCPU, pmap.CPUEwmaScore, oldMem, pmap.MemoryEwmaScore)
}

