package prober

import (
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"os"
	"time"

	"github.com/rs/zerolog/log"
)

// data struct for /scores prober endpoint
type ScoreData struct {
	Hostname         string  `json:"hostname"`
	CPUEwmaScore     float64 `json:"cpuEwmaScore"`
	LatencyEwmaScore float64 `json:"latencyEwmaScore"`
	MemoryEwmaScore  float64 `json:"memoryEwmaScore"`
}

func resolveNodeAddr(node string) string { //convert hostname to IP
	addrs, err := net.LookupHost(node)
	if err != nil || len(addrs) == 0 {
		return node 
	}
	// log.Debug().Msgf("[PROBER] Resolved %s → %s", node, addrs[0]) //deactivate logging for now
	return addrs[0]
}

func FetchScoresFromNode(node string) ([]ScoreData, error) {
	proberEndpoint := os.Getenv("PROBER_ENDPOINT")
	proberPort := os.Getenv("PROBER_PORT")

	nodeIP := resolveNodeAddr(node)
	url := fmt.Sprintf("http://%s:%s/%s", nodeIP, proberPort, proberEndpoint)
	log.Debug().Msgf("[PROBER] Fetching metrics from top node %s (%s)", url, node)

	client := http.Client{Timeout: 5 * time.Second}
	resp, err := client.Get(url)
	if err != nil {
		return nil, fmt.Errorf("[PROBER] failed to connect to prober on %s: %v", node, err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("[PROBER] unexpected response from prober %s: %s", node, resp.Status)
	}

	var scores []ScoreData
	if err := json.NewDecoder(resp.Body).Decode(&scores); err != nil {
		return nil, fmt.Errorf("[PROBER] failed to parse JSON from prober %s: %v", node, err)
	}

	// log.Info().Msgf("[PROBER] Retrieved %d score(s) from %s", len(scores), nodeIP)
	// return scores, nil

	for _, score := range scores {
		log.Debug().Msgf("[PROBER][SUMMARY] Node %s, cpu: %.3f, latency: %.3f, memory: %.3f",
			score.Hostname, score.CPUEwmaScore, score.LatencyEwmaScore, score.MemoryEwmaScore)
	}
	return scores, nil
}
