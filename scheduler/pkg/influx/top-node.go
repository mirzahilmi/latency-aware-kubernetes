package influx

import (
	"context"
	"fmt"
	"math"

	"github.com/rs/zerolog/log"
)

func (s *Service) QueryTopNode(bucket string) (string, float64, error) {
	if s.client == nil || s.queryAPI == nil {
		return "", 0, fmt.Errorf("[INFLUX] client or queryAPI not initialized")
	}

	query := fmt.Sprintf(`
from(bucket: "%s")
  |> range(start: -2m)
  |> filter(fn: (r) => r["_measurement"] == "http_packet")
  |> filter(fn: (r) => r["_field"] == "counter")
  |> aggregateWindow(every: 1m, fn: last, createEmpty: false)
  |> derivative(unit: 1m, nonNegative: true)
  |> group(columns: ["node_name"])
  |> sum(column: "_value")
  |> group()
  |> sort(columns: ["_value"], desc: true)
  |> limit(n: 1)
`, bucket)

	result, err := s.queryAPI.Query(context.Background(), query)
	if err != nil {
		return "", 0, fmt.Errorf("[INFLUX] query error: %v", err)
	}

	defer result.Close()

	var topNode string
	var reqRate float64

	for result.Next() {
		record := result.Record()

		node, _ := record.ValueByKey("node_name").(string)
		if node == "" {
			node, _ = record.ValueByKey("node").(string)
		}
		if node == "" {
			node, _ = record.ValueByKey("host").(string)
		}

		value, _ := record.Value().(float64)

		if node == "" || math.IsNaN(value) || value == 0 {
			log.Debug().Msgf("[INFLUX] Skip node %s: invalid or zero value (%.2f)", node, value)
			continue
		}

		topNode = node
		reqRate = value
	}

	if err := result.Err(); err != nil {
		return "", 0, fmt.Errorf("[INFLUX] result iteration error: %v", err)
	}

	if topNode == "" {
		log.Warn().Msg("[INFLUX] No data found in recent range, returning dummy node")
		return "none", 0, nil
	}

	log.Info().Msgf("[INFLUX][TOP-NODE] Top node = %s (%.2f req/min)", topNode, reqRate)
	return topNode, reqRate, nil
}
