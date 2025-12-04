package descheduler

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
)

type Summary struct {
	Node NodeStats `json:"node"`
	Pods []PodStats `json:"pods"`
}

type NodeStats struct {
	CPU    CPUStats    `json:"cpu"`
	Memory MemoryStats `json:"memory"`
}

type PodStats struct {
	PodRef     PodRef         `json:"podRef"`
	CPU        CPUStats       `json:"cpu"`
	Memory     MemoryStats    `json:"memory"`
	Containers []ContainerStats `json:"containers"`
}

type PodRef struct {
	Name      string `json:"name"`
	Namespace string `json:"namespace"`
}

type CPUStats struct {
	UsageNanoCores uint64 `json:"usageNanoCores"`
}

type MemoryStats struct {
	WorkingSetBytes uint64 `json:"workingSetBytes"`
}

type ContainerStats struct {
	Name   string    `json:"name"`
	CPU    CPUStats  `json:"cpu"`
	Memory MemoryStats `json:"memory"`
}

func (d *AdaptiveDescheduler) fetchSummaryAPI(nodeIP string) (*Summary, error) {
	url := fmt.Sprintf("https://%s:10250/stats/summary", nodeIP)

	req, err := http.NewRequest("GET", url, nil)
	if err != nil {
		return nil, err
	}

	// use Bearer token for authentication if available
	if d.kubeToken != "" {
		req.Header.Set("Authorization", "Bearer "+d.kubeToken)
	}

	transport := http.DefaultTransport.(*http.Transport).Clone()
	transport.TLSClientConfig = d.kubeTLSConfig

	client := &http.Client{Transport: transport}

	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("failed summary API: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("summary API status=%d body=%s", resp.StatusCode, string(body))
	}

	body, _ := io.ReadAll(resp.Body)

	var summary Summary
	if err := json.Unmarshal(body, &summary); err != nil {
		return nil, fmt.Errorf("unmarshal summary: %w", err)
	}

	return &summary, nil
}

// GetPodUsageFromSummary extracts total CPU (millicores) and Memory (MiB)
// for a given pod from the Kubelet Summary API response.
func getPodUsageFromSummary(podName string, podNS string, summary *Summary) (float64, float64, bool) {
	for _, p := range summary.Pods {
		if p.PodRef.Name == podName && p.PodRef.Namespace == podNS {
			var cpuNano uint64
			var memBytes uint64

			for _, c := range p.Containers {
				cpuNano += c.CPU.UsageNanoCores
				memBytes += c.Memory.WorkingSetBytes
			}

			cpuMilli := float64(cpuNano) / 1_000_000          // nanocores → millicores
			memMi := float64(memBytes) / (1024 * 1024)       // bytes → MiB

			return cpuMilli, memMi, true
		}
	}

	return 0, 0, false
}
