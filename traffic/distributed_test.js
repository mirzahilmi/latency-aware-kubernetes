import http from "k6/http";
import { sleep, check } from "k6";

const targets = __ENV.TARGETS.split(",");
const distributions = __ENV.DISTRIBUTIONS.split(",");

if (targets.length != distributions.length)
  throw "TARGETS and DISTRIBUTIONS count does not match!";

var scenarios = {};
for (let i = 0; i < targets.length; i++) {
  scenarios[`node-${i + 1}`] = {
      executor: "constant-vus",
      vus: Number(distributions[i]),
      duration: __ENV.DURATION_EACH,
      env: { TARGET: targets[i] },
  };
}

export const options = {
  scenarios: scenarios
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET}:30000`);
  check(res, { "status is 200": (res) => res.status === 200 });
}
