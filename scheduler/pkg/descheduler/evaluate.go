package descheduler

import (
	"context"
	"math"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/influx"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/scheduler"
	"github.com/rs/zerolog/log"

	"k8s.io/client-go/kubernetes"
	metricsclient "k8s.io/metrics/pkg/client/clientset/versioned"
)

func NewAdaptiveDescheduler(
	kube kubernetes.Interface,
	metrics metricsclient.Interface,
	influxSvc *influx.Service,
	bucket string,
	ns string,
	scoringCfg scheduler.ScoringConfig,
	deschedCfg DeschedulerConfig,
) *AdaptiveDescheduler {
	return &AdaptiveDescheduler{
		kubeClient: kube,
		metricsClient: metrics,
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
	// 1.) detect top node by querying node traffics
	topNode, rate, err := d.influxService.QueryTopNode(d.bucket)
	if err != nil {
		log.Error().Err(err).Msg("[DESCHEDULER] Failed to query topNode")
		return
	}
	if topNode == "" {
		log.Warn().Msg("[DESCHEDULER] No topNode detected — skipping evaluation")
		return
	}
	log.Info().Msgf("[DESCHEDULER] Current topNode: %s (%.2f req/min)", topNode, rate)

	//condition check, descheduler will evict pod if:
	//2. new top node != previous top node
	if topNode == d.prevTopNode {
        log.Debug().Msg("[DESCHEDULER] Traffic stable, nothing to do")
        return
    }
    log.Warn().Msgf("[DESCHEDULER] Traffic shift detected %s → %s", d.prevTopNode, topNode)

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

	// 4. compute node scores
	scores := make(map[string]float64)
    for _, s := range pmap {
        node := s.Hostname
        scores[node] = float64(scheduler.ScoreNode(node, map[string]prober.ScoreData{node: s}, nmap, d.scoringCfg))
    }

	// 5. find the worst node to evict from
	worstNode, lowestScore := "", math.MaxFloat64
	for node, score := range scores {
		if score < lowestScore {
			worstNode, lowestScore = node, score
		}
	}
	if worstNode == "" {
		log.Warn().Msg("[DESCHEDULER] No target node selected, skipping eviction")
		return
	}
	log.Warn().Msgf("[DESCHEDULER] Worst node identified: %s (score=%.2f)", worstNode, lowestScore)

	// 6. evict idle pod from worst node,
	if err := d.evictIdlePod(ctx, worstNode); err != nil {
		log.Warn().Err(err).Msgf("[DESCHEDULER] Eviction failed for %s", worstNode)
	} 
	
	// 7. update previous top node
	d.prevTopNode = topNode
}

