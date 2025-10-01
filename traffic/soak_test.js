import http from 'k6/http';
import { sleep, check } from 'k6';

export const options = {
  stages: [
    { duration: "1m", target: 50 },
    { duration: "30s", target: 50 },
    { duration: "50s", target: 150 },
    { duration: "3m", target: 200 },
    { duration: "2m", target: 300 },
    { duration: "24h", target: 300 },
  ],
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET_HOSTNAME}:30000`);
  check(res, { "status is 200": (res) => res.status === 200 });
  sleep(1);
}
