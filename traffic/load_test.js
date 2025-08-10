import http from 'k6/http';
import { sleep, check } from 'k6';

export const options = {
  stages: [
    { duration: "10s", target: 20 },
    { duration: "10s", target: 50 },
    { duration: "60s", target: 50 },
    { duration: "50s", target: 20 },
  ],
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET_HOSTNAME}:8000`);
  check(res, { "status is 200": (res) => res.status === 200 });
  sleep(1);
}
