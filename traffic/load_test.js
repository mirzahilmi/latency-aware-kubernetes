import http from 'k6/http';
import { sleep, check } from 'k6';

export const options = {
  stages: [
    { duration: "10s", target: 100 },
    { duration: "10s", target: 100 },
    { duration: "20s", target: 2000 },
    { duration: "10s", target: 2000 },
    { duration: "10s", target: 400 },
    { duration: "10s", target: 400 },
    { duration: "30s", target: 3000 },
    { duration: "40s", target: 3000 },
    { duration: "20s", target: 1000 },
    { duration: "40s", target: 7000 },
    { duration: "30s", target: 2000 },
    { duration: "20s", target: 2000 },
    { duration: "40s", target: 700 },
    { duration: "20s", target: 0 },
  ],
};

export default function() {
  const res = http.get(`http://${__ENV.HELLOPOD_HOSTNAME}:8000`);
  check(res, { "status is 200": (res) => res.status === 200 });
  sleep(1);
}
