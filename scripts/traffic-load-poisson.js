import http from "k6/http";
import { SharedArray } from 'k6/data';
import { check } from "k6";

// Poisson variant of traffic-load.js.
//
// Same open-model semantics and the same mean load: DISTRIBUTIONS[i] is the
// arrival rate lambda (req/s) for nodes[i]. Instead of k6 pacing arrivals
// perfectly evenly, the request count of every tick (default 1s) is drawn from
// Poisson(lambda * tick), i.e. a homogeneous Poisson process observed per tick.
// The sampled per-tick rates are emitted as constant ramping-arrival-rate
// segments, so k6 keeps reporting dropped iterations under saturation.
//
// The PRNG is seeded (k6's Math.random cannot be), so a given
// SEED + DISTRIBUTIONS + DURATION + TICK always replays the exact same arrival
// trace. Run baseline and latency-aware scheduler with the same SEED to keep
// the traffic identical across comparisons.
//
// Env:
//   DISTRIBUTIONS  required  comma separated mean req/s, positional to config.json nodes[]
//   DURATION       required  k6 duration, e.g. "7m"
//   SEED           optional  integer, default 42
//   TICK           optional  resampling interval, default "1s"

const nodes = new SharedArray("nodes", function () {
  return JSON.parse(open("./config.json")).nodes;
});
const distributions = __ENV.DISTRIBUTIONS.split(",");

if (nodes.length == 0 || distributions.length == 0)
  throw "NODES or DISTRIBUTIONS IS EMPTY";

if (distributions.length > nodes.length)
  throw "DISTRIBUTIONS HAS MORE ENTRIES THAN config.json nodes[]";

// mulberry32: small seedable PRNG, uniform in [0,1).
function mulberry32(seed) {
  let a = seed >>> 0;
  return function () {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// Lanczos approximation, needed by the rejection step below.
const LANCZOS = [
  0.99999999999980993, 676.5203681218851, -1259.1392167224028,
  771.32342877765313, -176.61502916214059, 12.507343278686905,
  -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7,
];

function logGamma(x) {
  const z = x - 1;
  const t = z + 7.5;
  let a = LANCZOS[0];
  for (let i = 1; i < LANCZOS.length; i++) a += LANCZOS[i] / (z + i);
  return 0.5 * Math.log(2 * Math.PI) + (z + 0.5) * Math.log(t) - t + Math.log(a);
}

// Knuth for small lambda, Hoermann's PTRS transformed rejection above 30
// (exp(-lambda) underflows there, and PTRS is O(1) instead of O(lambda)).
function samplePoisson(rand, lambda) {
  if (lambda <= 0) return 0;

  if (lambda < 30) {
    const limit = Math.exp(-lambda);
    let k = 0;
    let p = 1;
    do {
      k++;
      p *= rand();
    } while (p > limit);
    return k - 1;
  }

  const b = 0.931 + 2.53 * Math.sqrt(lambda);
  const a = -0.059 + 0.02483 * b;
  const invAlpha = 1.1239 + 1.1328 / (b - 3.4);
  const vr = 0.9277 - 3.6224 / (b - 2);
  const logLambda = Math.log(lambda);

  for (;;) {
    const u = rand() - 0.5;
    const v = rand();
    const us = 0.5 - Math.abs(u);
    const k = Math.floor(((2 * a) / us + b) * u + lambda + 0.43);

    if (us >= 0.07 && v <= vr) return k;
    if (k < 0 || (us < 0.013 && v > us)) continue;
    if (Math.log((v * invAlpha) / (a / (us * us) + b)) <=
        k * logLambda - lambda - logGamma(k + 1))
      return k;
  }
}

function parseDurationSeconds(value) {
  const text = String(value).trim();
  if (/^\d+(\.\d+)?$/.test(text)) return Number(text);

  const units = { h: 3600, m: 60, s: 1, ms: 0.001 };
  const re = /(\d+(?:\.\d+)?)(ms|h|m|s)/g;
  let total = 0;
  let consumed = 0;
  let part;
  while ((part = re.exec(text)) !== null) {
    total += Number(part[1]) * units[part[2]];
    consumed += part[0].length;
  }
  if (consumed !== text.length || total <= 0) throw `INVALID DURATION: ${value}`;
  return total;
}

function buildScenarios() {
  const tickText = __ENV.TICK || "1s";
  const tickSeconds = parseDurationSeconds(tickText);
  const totalSeconds = parseDurationSeconds(__ENV.DURATION);
  const ticks = Math.max(1, Math.round(totalSeconds / tickSeconds));
  const seed = Number(__ENV.SEED || 42);
  if (!Number.isFinite(seed)) throw `INVALID SEED: ${__ENV.SEED}`;

  const scenarios = {};
  for (let i = 0; i < distributions.length; i++) {
    const lambda = Number(distributions[i]);
    if (!(lambda > 0)) throw `INVALID DISTRIBUTION AT INDEX ${i}: ${distributions[i]}`;

    // One independent stream per node, still fully determined by SEED.
    const rand = mulberry32(seed + i * 0x9e3779b1);

    const rates = [];
    for (let t = 0; t < ticks; t++)
      rates.push(Math.round(samplePoisson(rand, lambda * tickSeconds) / tickSeconds));

    // Piecewise constant: instantaneous jump (0s) then hold for one tick.
    const stages = [{ target: rates[0], duration: tickText }];
    for (let t = 1; t < ticks; t++) {
      stages.push({ target: rates[t], duration: "0s" });
      stages.push({ target: rates[t], duration: tickText });
    }

    let maxRate = 0;
    for (let t = 0; t < rates.length; t++)
      if (rates[t] > maxRate) maxRate = rates[t];
    const vus = Math.max(1, maxRate * 3);

    scenarios[nodes[i].hostname] = {
      executor: "ramping-arrival-rate",
      startRate: rates[0],
      timeUnit: "1s",
      stages: stages,
      preAllocatedVUs: vus,
      maxVUs: vus,
      env: { TARGET: nodes[i].ip },
    };
  }

  return scenarios;
}

// The init context re-runs for every VU, and the schedule can hold thousands of
// stages. Only the first pass (__VU == 0) is read for options, so build it once
// and hand VUs an empty object instead of a per-VU copy.
export const options = {
  scenarios: __VU == 0 ? buildScenarios() : {},
  discardResponseBodies: true,
  noVUConnectionReuse: false,
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET}:30000`, { timeout: "300s" });
  check(res, { "status is 200": (res) => res.status === 200 });
}
