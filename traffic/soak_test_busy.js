import http from 'k6/http';
import { sleep, check } from 'k6';

export const options = {
  stages: [
    { duration: "1m", target: 50 },
    { duration: "30s", target: 100 },
    { duration: "2m", target: 100 },
    { duration: "1m", target: 200 },
    { duration: "5m", target: 200 },
    { duration: "2m", target: 400 },
    { duration: "10m", target: 400 },
    { duration: "8m", target: 800 },
    { duration: "1h30m", target: 800 },
    { duration: "30m", target: 1600 },
    { duration: "1h", target: 1600 },
    { duration: "30m", target: 3800 },
    { duration: "1h", target: 3800 },
    { duration: "30m", target: 5000 },
    { duration: "3h", target: 5000 },
    { duration: "20m", target: 2500 },
    { duration: "5m", target: 2500 },
    { duration: "10m", target: 100 },
  ],
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET_HOSTNAME}:30000`);
  check(res, { "status is 200": (res) => res.status === 200 });
  sleep(1);
}
