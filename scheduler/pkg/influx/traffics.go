package influx

import (
	"context"
	"fmt"

	"github.com/rs/zerolog/log"
)

// QueryTrafficByNode returns map[node_name] = req_per_min
func (s *Service) QueryTrafficByNode(bucket string) (map[string]float64, error) {
	if s.client == nil || s.queryAPI == nil {
		return nil, fmt.Errorf("[INFLUX] client or queryAPI not initialized")
	}

	query := fmt.Sprintf(`
from(bucket: "%s")
  |> range(start: -1m)
  |> filter(fn: (r) => r["_measurement"] == "http_packet")
  |> filter(fn: (r) => r["_field"] == "counter")
  |> aggregateWindow(every: 1m, fn: last)
  |> derivative(unit: 1m, nonNegative: true)
  |> group(columns: ["node_name"])
  |> sum()
`, bucket)

	result, err := s.queryAPI.Query(context.Background(), query)
	if err != nil {
		return nil, fmt.Errorf("[INFLUX] query error: %v", err)
	}
	defer result.Close()

	trafficMap := make(map[string]float64)
	for result.Next() {
		node, _ := result.Record().ValueByKey("node_name").(string)
		value, _ := result.Record().Value().(float64)
		if node != "" {
			trafficMap[node] = value
		}
	}

	if err := result.Err(); err != nil {
		return nil, fmt.Errorf("[INFLUX] result iteration error: %v", err)
	}

	if len(trafficMap) == 0 {
		log.Warn().Msg("[INFLUX] QueryTrafficByNode returned no records (empty traffic map)")
	} else {
		log.Info().Msgf("[INFLUX] Retrieved traffic map for %d nodes", len(trafficMap))
	}

	return trafficMap, nil
}
