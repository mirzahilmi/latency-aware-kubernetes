package descheduler

import (
	"context"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/scheduler"
	"github.com/rs/zerolog/log"

	"k8s.io/client-go/kubernetes"
	"k8s.io/client-go/rest"
)
func NewAdaptiveDescheduler(
	kube kubernetes.Interface,
	restCfg *rest.Config,
	influxSvc *influx.Service,
	bucket string,
	ns string,
	scoringCfg scheduler.ScoringConfig,
	deschedCfg DeschedulerConfig,
) *AdaptiveDescheduler {
	tlsCfg, err := rest.TLSConfigFor(restCfg)
	if err != nil {
		log.Fatal().Err(err).Msg("[DESCHEDULER] Failed to create TLS config from kube rest config")
	}

	return &AdaptiveDescheduler{
		kubeClient: kube,
		kubeTLSConfig: tlsCfg,
		kubeToken:     restCfg.BearerToken,
		influxService: influxSvc,
		bucket: bucket,
		namespace: ns,
		scoringCfg: scoringCfg,
		deschedCfg: deschedCfg,
	}
}

func (d *AdaptiveDescheduler) evaluate(ctx context.Context) {
    d.mu.Lock()
    defer d.mu.Unlock()

    d.evaluateCycle(ctx)
}

func (d *AdaptiveDescheduler) evaluateCycle(ctx context.Context) {
	// 1. detect top node by querying node traffics
	topNode, rate, err := d.influxService.QueryTopNode(d.bucket)
	if err != nil {
		log.Warn().Err(err).Msg("[DESCHEDULER] Failed to query top node")
		return
	}
	if topNode == "" {
		log.Info().Msg("[DESCHEDULER] No traffic data available, skipping descheduling")
		return
	}
	log.Info().Msgf("[DESCHEDULER] New top node: %s (%.2f req/min)", topNode, rate)

	//2. new top node != previous top node
	if topNode == d.prevTopNode {
		log.Info().Msgf("[DESCHEDULER] Top node unchanged: %s (%.2f req/min), no action needed", topNode, rate)
		return
	}
    log.Warn().Msgf("[DESCHEDULER] Traffic shift detected %s → %s", d.prevTopNode, topNode)
	// if topNode != d.prevTopNode {
	// 	log.Warn().Msgf("[DESCHEDULER] Traffic shift detected %s → %s", d.prevTopNode, topNode)
	// } else {
	// 	log.Info().Msgf("[DESCHEDULER] Top node unchanged: %s (%.2f req/min)", topNode, rate)
	// }
	// log.Info().Msgf("[DESCHEDULER] Current top node: %s (%.2f req/min)", topNode, rate)

	// 3. fetch metrics from top node (if there's a traffic shift)
	pmap, err := prober.FetchScoresFromNode(topNode)
	if err != nil || len(pmap) == 0 {
		log.Warn().Err(err).Msgf("[DESCHEDULER] Failed to fetch prober data from %s", topNode)
		return
	}

	nmap, err := d.influxService.NormalizedTraffic(d.bucket)
	if err != nil {
		log.Warn().Err(err).Msg("[DESCHEDULER] Failed to fetch normalized traffic")
		nmap = map[string]float64{}
	}

	// 4. compute all node' scores
	scores := make(map[string]float64)
    for _, s := range pmap {
        node := s.Hostname
        score := scheduler.ScoreNode(
			node,
			map[string]prober.ScoreData{node: s},
			nmap,
			d.scoringCfg,
		)
		scores[node] = float64(score)

		log.Debug().Msgf(
			"[SCORING] Node %s score=%.2f (CPU=%.3f Lat=%.3f Traffic=%.3f)",
			node, float64(score), s.CPUEwmaScore, s.LatencyEwmaScore, nmap[node],
		)
    }

	// 5. find the worst node to evict from
	ranked := d.scoreNodes(pmap, nmap)
	if len(ranked) == 0 {
		log.Warn().Msg("[DESCHEDULER] No node scores computed, skipping")
		return
	}

	log.Info().Msg("[DESCHEDULER] Node ranking (ascending by score):")
	for _, ns := range ranked {
		log.Info().Msgf("  - %s: score=%.2f", ns.Name, ns.Score)
	}

	//6. try evicting idle pod from worst node
	var evictionDone bool

	for _, ns := range ranked {
		nodeName := ns.Name

		log.Warn().Msgf("[DESCHEDULER] Trying candidate node for eviction: %s (score=%.2f)", nodeName, ns.Score)

		ok, err := d.evictIdlePod(ctx, nodeName)
		if err != nil {
			log.Warn().Err(err).Msgf("[DESCHEDULER] Eviction attempt failed on node %s, trying next candidate", nodeName)
			continue
		}
		if ok {
			log.Info().Msgf("[DESCHEDULER] Successfully evicted idle pod from node %s", nodeName)
			evictionDone = true
			break
		}
		log.Info().Msgf("[DESCHEDULER] No idle pod to evict on node %s, trying next candidate", nodeName)
	}

	if !evictionDone {
		log.Warn().Msg("[DESCHEDULER] No idle pod found on any low-score node, skipping this cycle")
	}

	// 7. update previous top node
	d.prevTopNode = topNode



	// worstNode, lowestScore := "", math.MaxFloat64
	// for node, score := range scores {
	// 	if score < lowestScore {
	// 		worstNode, lowestScore = node, score
	// 	}
	// }
	// if worstNode == "" {
	// 	log.Warn().Msg("[DESCHEDULER] No target node selected, skipping eviction")
	// 	return
	// }
	// log.Warn().Msgf("[DESCHEDULER] Worst node identified: %s (score=%.2f)", worstNode, lowestScore)

	// // 6. evict idle pod from worst node,
	// if err := d.evictIdlePod(ctx, worstNode); err != nil {
	// 	log.Warn().Err(err).Msgf("[DESCHEDULER] Eviction failed for %s", worstNode)
	// }
 }
	