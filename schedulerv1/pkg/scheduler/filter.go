package scheduler

import (
	"encoding/json"
	"net/http"

	"github.com/rs/zerolog/log"
	extenderv1 "k8s.io/kube-scheduler/extender/v1"
	v1 "k8s.io/api/core/v1"
)

// handles /filter request from the scheduler (pkg/scheduler/routes.go)
func (e *Extender) Filter(w http.ResponseWriter, r *http.Request) {
	var args extenderv1.ExtenderArgs

	if err := json.NewDecoder(r.Body).Decode(&args); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	podName := args.Pod.Name
	nodeList := args.Nodes.Items

	log.Info().
		Str("[FILTER] pod", podName).
		Int("[FILTER] nodeCount", len(nodeList)).
		Msg("[FILTER] Filter request received")

	e.refreshProberData()

	e.mu.RLock()
	pmap := e.proberScores
	e.mu.RUnlock()

	failedNodes := make(map[string]string)
	filteredNodes := []v1.Node{}

	if len(pmap) == 0 {
		log.Warn().Msg("[FILTER] No prober data available, passing all nodes")
		filteredNodes = nodeList
	} else {
		for _, node := range nodeList {
			nodes := node.Name
			ps, ok := pmap[nodes]
			
			if !ok {
				failedNodes[nodes] = "no prober data"
				log.Debug().Str("node", nodes).Msg("[FILTER] Rejected: no metrics")
				continue
			}

			if ps.LatencyEwmaScore < e.cfg.LatencyThreshold {
				failedNodes[nodes] = "latency below threshold"
				log.Debug().Msgf("[FILTER] %s REJECTED: latency=%.3f < %.2f",
					nodes, ps.LatencyEwmaScore, e.cfg.LatencyThreshold)
				continue
			}

			if ps.CPUEwmaScore < e.cfg.cpuThreshold && ps.MemoryEwmaScore < e.cfg.memThreshold {
				failedNodes[nodes] = "cpu & memory below threshold"
				log.Debug().Msgf("[FILTER] %s REJECTED (cpu=%.3f < %.2f, memory=%.3f < %.2f)",
					nodes, ps.CPUEwmaScore, e.cfg.cpuThreshold, ps.MemoryEwmaScore, e.cfg.memThreshold)
				continue
			}

			// Node passed all threshold
			filteredNodes = append(filteredNodes, node)
			log.Debug().Msgf("[FILTER] %s PASSED (cpu=%.3f latency=%.3f memory=%.3f)",
				nodes, ps.CPUEwmaScore, ps.LatencyEwmaScore, ps.MemoryEwmaScore)
		}
	}

	result := extenderv1.ExtenderFilterResult{
		Nodes: &v1.NodeList{
			Items: filteredNodes,
		},
		FailedNodes: failedNodes,
		Error:       "",
	}

	log.Info().
		Int("passed", len(filteredNodes)).
		Int("failed", len(failedNodes)).
		Msg("[FILTER] Phase completed")

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(result)
}

