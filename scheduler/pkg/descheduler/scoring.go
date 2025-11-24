package descheduler

import (
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/scheduler"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
	"github.com/rs/zerolog/log"
)

// ScoreAllNodes count all node scores using extender.ScoreNode() formula
func (d *AdaptiveDescheduler) ScoreAllNodes(
	pmap []prober.ScoreData,
	nmap map[string]float64,
	schedulerCfg scheduler.ScoringConfig,
) map[string]float64 {
	scores := make(map[string]float64)
	for _, s := range pmap {
		nodeName := s.Hostname
		score := scheduler.ScoreNode(
			nodeName,
			map[string]prober.ScoreData{nodeName: s},
			nmap,
			schedulerCfg,
		)
		scores[nodeName] = float64(score)
		log.Debug().Msgf(
			"[SCORING] Node %s score=%.2f (CPU=%.3f Lat=%.3f Traffic=%.3f)",
			nodeName, float64(score), s.CPUEwmaScore, s.LatencyEwmaScore, nmap[nodeName],
		)
	}
	return scores
}
