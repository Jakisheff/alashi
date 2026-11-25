#!/usr/bin/env python3
"""Single-process ES trainer for the alashi-rules reward experiment.

One process owns the model: an HTTP thread serves /v1/chat/completions
(greedy, temperature 0) for the world driver's focal agent; the training
loop perturbs/updates the same in-process weights under a lock.

Pre-registered design (2026-11-25, alashi docs/research/preprint-notes):
N=30, sigma=0.001, alpha=5e-4; reward = mean focal payoff over K episodes
of a 6-agent world (focal model vs five uniform scripted opponents via
mixed-provider.mjs). Metrics recorded per iteration: payoff, invalid
actions, bribes (the world-100 pathology set).

Usage (GPU box, cwd tools/agent-bootstrap):
  node mixed-provider.mjs 7780 &     # ES_MODEL_URL -> this server's port
  python3 es-train.py Qwen/Qwen2.5-0.5B-Instruct 100

Requires: world_worker built; es-reward.mjs; mixed-provider.mjs running
with ES_MODEL_URL=http://127.0.0.1:7779/v1/chat/completions.
"""
import json
import subprocess
import sys
import threading
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

MODEL_NAME = sys.argv[1] if len(sys.argv) > 1 else "Qwen/Qwen2.5-0.5B-Instruct"
ITERATIONS = int(sys.argv[2]) if len(sys.argv) > 2 else 100
PORT = 7779
POPULATION = 30
SIGMA = 0.001
ALPHA = 5e-4
EPISODES_PER_MEMBER = 2
BASE_DIR = "/tmp/es-worlds"

device = "cuda" if torch.cuda.is_available() else "cpu"
tokenizer = AutoTokenizer.from_pretrained(MODEL_NAME)
model = AutoModelForCausalLM.from_pretrained(MODEL_NAME, torch_dtype=torch.bfloat16).to(device)
model.eval()
LOCK = threading.Lock()
PAUSED = threading.Event()  # set while the loop applies the aggregate update


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/v1/chat/completions":
            self.send_error(404)
            return
        length = int(self.headers.get("Content-Length", 0))
        data = json.loads(self.rfile.read(length))
        messages = data.get("messages", [])
        while PAUSED.is_set():
            PAUSED.wait()
        with LOCK, torch.no_grad():
            prompt = tokenizer.apply_chat_template(messages, tokenize=False,
                                                    add_generation_prompt=True, enable_thinking=False)
            inputs = tokenizer(prompt, return_tensors="pt").to(device)
            out = model.generate(**inputs, max_new_tokens=256, do_sample=False,
                                 temperature=None, top_p=None, top_k=None)
            text = tokenizer.decode(out[0][inputs["input_ids"].shape[1]:], skip_special_tokens=True)
        n_in = inputs["input_ids"].shape[1]
        n_out = out.shape[1] - n_in
        body = json.dumps({
            "model": data.get("model", MODEL_NAME),
            "choices": [{"message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": n_in, "completion_tokens": n_out, "total_tokens": n_in + n_out},
        })
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(body.encode())

    def log_message(self, *_):
        pass


def layer_params():
    for name, layer in model.named_modules():
        params = [(pname, p) for pname, p in layer.named_parameters(recurse=False)]
        if params:
            yield name, params


def noise_for(seed):
    generator = torch.Generator(device="cpu").manual_seed(seed)
    return {name: {pname: torch.randn(p.shape, generator=generator)
                   for pname, p in params} for name, params in layer_params()}


def perturb(sigma, seed):
    with LOCK, torch.no_grad():
        noises = noise_for(seed)
        for name, params in layer_params():
            for pname, p in params:
                p.add_((noises[name][pname].to(p.dtype).to(p.device)) * sigma)
    return noises


def restore(noises, sigma):
    with LOCK, torch.no_grad():
        for name, params in layer_params():
            for pname, p in params:
                p.sub_((noises[name][pname].to(p.dtype).to(p.device)) * sigma)


def episode(seed):
    out = subprocess.run(["node", "es-reward.mjs", f"{BASE_DIR}/e{seed}", str(seed)],
                         capture_output=True, text=True, check=True)
    return json.loads(out.stdout.strip().splitlines()[-1])


def zscore(values):
    mean = sum(values) / len(values)
    std = (sum((v - mean) ** 2 for v in values) / len(values)) ** 0.5 or 1.0
    return [(v - mean) / std for v in values]


def payoff_of(record):
    return (record.get("focal_payout") or 0) + (record.get("focal_cash") or 0) / 10_000_000_000


def main():
    server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    print(f"serving model on 127.0.0.1:{PORT}; training {ITERATIONS} iterations", file=sys.stderr)
    log_path = f"{BASE_DIR}/es-train-log.jsonl"
    for t in range(ITERATIONS):
        rewards, invalids = [], []
        for n in range(POPULATION):
            seed = t * POPULATION + n + 1
            noises = perturb(SIGMA, seed)
            member_rewards, member_invalids = [], []
            for k in range(EPISODES_PER_MEMBER):
                record = episode(seed * 1000 + k)
                member_rewards.append(payoff_of(record))
                member_invalids.append(record.get("focal_invalid") or 0)
            restore(noises, SIGMA)
            rewards.append(sum(member_rewards) / len(member_rewards))
            invalids.append(sum(member_invalids) / len(member_invalids))
            print(f"iter {t} member {n}: reward={rewards[-1]:.2f} invalid={invalids[-1]}", file=sys.stderr)
        weights = zscore(rewards)
        PAUSED.set()
        with LOCK, torch.no_grad():
            for n, seed in enumerate(range(t * POPULATION + 1, t * POPULATION + 1 + POPULATION)):
                noises = noise_for(seed)
                for name, params in layer_params():
                    for pname, p in params:
                        p.add_((noises[name][pname].to(p.dtype).to(p.device)) * (ALPHA * weights[n] / POPULATION))
        PAUSED.clear()
        with open(log_path, "a") as log:
            log.write(json.dumps({"iter": t, "mean_reward": sum(rewards) / len(rewards),
                                  "best_reward": max(rewards), "mean_invalid": sum(invalids) / len(invalids)}) + "\n")
        print(f"iter {t}: mean={sum(rewards)/len(rewards):.2f} best={max(rewards):.2f}", file=sys.stderr)


if __name__ == "__main__":
    main()
