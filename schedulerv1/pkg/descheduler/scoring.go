package descheduler

import (
	"sort"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/scheduler"
	"github.com/rs/zerolog/log"
)

// ScoreAllNodes count all node scores using extender.ScoreNode() formula
// func (d *AdaptiveDescheduler) scoreNodes(
// 	pmap []prober.ScoreData,
// 	nmap map[string]float64,
// 	schedulerCfg scheduler.ScoringConfig,
// ) map[string]float64 {
// 	scores := make(map[string]float64)
// 	for _, s := range pmap {
// 		nodeName := s.Hostname
// 		score := scheduler.ScoreNode(
// 			nodeName,
// 			map[string]prober.ScoreData{nodeName: s},
// 			nmap,
// 			schedulerCfg,
// 		)
// 		scores[nodeName] = float64(score)
// 		log.Debug().Msgf(
// 			"[SCORING] Node %s score=%.2f (CPU=%.3f Memory=%.3f Lat=%.3f Traffic=%.3f)",
// 			nodeName, float64(score), s.CPUEwmaScore, s.MemoryEwmaScore, s.LatencyEwmaScore, nmap[nodeName],
// 		)
// 	}
// 	return scores
// }

type NodeScore struct {
	Name  string
	Score float64
}

func (d *AdaptiveDescheduler) scoreNodes(
	pmap []prober.ScoreData,
	nmap map[string]float64,
) []NodeScore {
	scores := make([]NodeScore, 0, len(pmap))

	for _, s := range pmap {
		node := s.Hostname

		score := scheduler.ScoreNode(
			node,
			map[string]prober.ScoreData{node: s},
			nmap,
			d.scoringCfg,
		)

		ns := NodeScore{
			Name:  node,
			Score: float64(score),
		}
		scores = append(scores, ns)

		log.Debug().Msgf(
			"[SCORING] Node %s score=%.2f (CPU=%.3f Lat=%.3f Traffic=%.3f)",
			node, ns.Score, s.CPUEwmaScore, s.LatencyEwmaScore, nmap[node],
		)
	}

	sort.Slice(scores, func(i, j int) bool {
		return scores[i].Score < scores[j].Score
	})

	return scores
}