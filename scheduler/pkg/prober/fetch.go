package prober

import (
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"time"
)

// ScoreData merepresentasikan struktur data dari endpoint /scores prober
type ScoreData struct {
	Hostname         string  `json:"hostname"`
	CPUEwmaScore     float64 `json:"cpuEwmaScore"`
	LatencyEwmaScore float64 `json:"latencyEwmaScore"`
}

// FetchScoresFromNode mengambil data /scores dari node prober tertentu
func FetchScoresFromNode(node string) ([]ScoreData, error) {
	proberEndpoint := os.Getenv("PROBER_ENDPOINT")
	proberPort := os.Getenv("PROBER_PORT")

	url := fmt.Sprintf("http://%s:%s/%s", node, proberPort, proberEndpoint)

	client := http.Client{Timeout: 5 * time.Second}
	resp, err := client.Get(url)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to prober on %s: %v", node, err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("unexpected response from prober %s: %s", node, resp.Status)
	}

	var scores []ScoreData
	if err := json.NewDecoder(resp.Body).Decode(&scores); err != nil {
		return nil, fmt.Errorf("failed to parse JSON from prober %s: %v", node, err)
	}

	return scores, nil
}
