import http from "k6/http";
import { sleep, check } from "k6";

const targets = __ENV.TARGET_HOSTNAMES.split(",");

export const options = {
  scenarios: {
    node_1: {
      executor: "ramping-arrival-rate",
      preAllocatedVUs: 1000,
      env: { TARGET_HOSTNAME: targets.pop() },
      stages: [
        // Load
        { duration: "2m", target: 100 },
        { duration: "5m", target: 100 },
        { duration: "2m", target: 200 },
        { duration: "5m", target: 200 },
        // Stress
        { duration: "2m", target: 400 },
        { duration: "5m", target: 400 },
        { duration: "2m", target: 500 },
        { duration: "5m", target: 500 },
        { duration: "2m", target: 700 },
        { duration: "5m", target: 700 },
        // Spike
        { duration: "30s", target: 1000 },
        { duration: "1m", target: 1000 },
        { duration: "30s", target: 200 },
        { duration: "2m", target: 200 },
        { duration: "30s", target: 1000 },
        { duration: "1m", target: 1000 },
        { duration: "30s", target: 300 },
        { duration: "2m", target: 300 },
        // Soak
        { duration: "10m", target: 200 },
        { duration: "20h", target: 800 },
        { duration: "30m", target: 0 },
      ],
    },
    node_2: {
      executor: "ramping-arrival-rate",
      preAllocatedVUs: 1000,
      env: { TARGET_HOSTNAME: targets.pop() },
      startTime: "5m",
      stages: [
        // Spike
        { duration: "30s", target: 1000 },
        { duration: "1m", target: 1000 },
        { duration: "30s", target: 200 },
        { duration: "2m", target: 200 },
        { duration: "30s", target: 1000 },
        { duration: "1m", target: 1000 },
        { duration: "30s", target: 300 },
        { duration: "2m", target: 300 },
        // Stress
        { duration: "2m", target: 400 },
        { duration: "5m", target: 400 },
        { duration: "2m", target: 500 },
        { duration: "5m", target: 500 },
        { duration: "2m", target: 700 },
        { duration: "5m", target: 700 },
        // Load
        { duration: "2m", target: 100 },
        { duration: "5m", target: 100 },
        { duration: "2m", target: 200 },
        { duration: "5m", target: 200 },
        // Soak
        { duration: "10m", target: 200 },
        { duration: "20h", target: 800 },
        { duration: "30m", target: 0 },
      ],
    },
  },
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET_HOSTNAME}:8000`);
  check(res, { "status is 200": (res) => res.status === 200 });
  sleep(1);
}
