import http from "k6/http";
import { SharedArray } from "k6/data";
import { check } from "k6";

// Each node receives an independent Poisson process at its mean rate: the gaps
// between arrivals are exponentially distributed, and k6 replays the resulting
// number of arrivals in every second. The PRNG is seeded (k6's Math.random
// cannot be), so the same SEED replays the same arrivals for every environment.
//
// Env:
//   RATES             comma separated mean req/s, positional to config.json nodes[]
//   DURATION_SECONDS  length of the run
//   SEED              integer seed of the arrival trace

// VUs reserved per req/s of peak rate (Little's law). Every VU is a JS runtime,
// so a looser budget gets long runs OOM-killed.
const RESPONSE_TIME_BUDGET_SECONDS = 1;

const nodes = new SharedArray("nodes", function () {
  return JSON.parse(open("./config.json")).nodes;
});

// mulberry32, uniform in [0, 1)
function seededRandom(seed) {
  let state = seed >>> 0;
  return function () {
    state = (state + 0x6d2b79f5) >>> 0;
    let mixed = state;
    mixed = Math.imul(mixed ^ (mixed >>> 15), mixed | 1);
    mixed ^= mixed + Math.imul(mixed ^ (mixed >>> 7), mixed | 61);
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296;
  };
}

function exponentialGap(random, meanRate) {
  return -Math.log(1 - random()) / meanRate;
}

function poissonArrivalsPerSecond(random, meanRate, seconds) {
  const arrivals = new Array(seconds).fill(0);
  for (let time = exponentialGap(random, meanRate); time < seconds; time += exponentialGap(random, meanRate))
    arrivals[Math.floor(time)]++;
  return arrivals;
}

// Jump to each second's arrival count instantly, then hold it for that second.
function stagesHolding(arrivals) {
  const stages = [];
  for (const count of arrivals)
    stages.push({ target: count, duration: "0s" }, { target: count, duration: "1s" });
  return stages;
}

function buildScenarios() {
  const meanRates = __ENV.RATES.split(",").map(Number);
  const seconds = Number(__ENV.DURATION_SECONDS);
  const seed = Number(__ENV.SEED);

  if (meanRates.length > nodes.length)
    throw `RATES has ${meanRates.length} entries but config.json has ${nodes.length} nodes`;

  const scenarios = {};
  meanRates.forEach(function (meanRate, nodeIndex) {
    if (meanRate === 0) return;

    const arrivals = poissonArrivalsPerSecond(seededRandom(seed + nodeIndex), meanRate, seconds);
    const virtualUsers = Math.max(1, Math.ceil(Math.max(...arrivals) * RESPONSE_TIME_BUDGET_SECONDS));

    scenarios[nodes[nodeIndex].hostname] = {
      executor: "ramping-arrival-rate",
      startRate: arrivals[0],
      timeUnit: "1s",
      stages: stagesHolding(arrivals),
      preAllocatedVUs: virtualUsers,
      maxVUs: virtualUsers,
      env: { TARGET: nodes[nodeIndex].ip },
    };
  });
  return scenarios;
}

// Every VU re-runs this init code, but k6 only reads options from the first pass.
export const options = {
  scenarios: __VU == 0 ? buildScenarios() : {},
  discardResponseBodies: true,
  noVUConnectionReuse: true,
};

export default function () {
  const res = http.get(`http://${__ENV.TARGET}:30000`, { timeout: "300s" });
  check(res, { "status is 200": (res) => res.status === 200 });
}
