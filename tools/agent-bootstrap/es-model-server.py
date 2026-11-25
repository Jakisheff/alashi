#!/usr/bin/env python3
"""Loopback OpenAI-compatible inference server for the ES experiment.

Serves /v1/chat/completions from an in-process transformers model whose
weights the ES loop perturbs in place between episodes. Greedy decoding,
temperature 0 — all performance differences come from parameter-space
perturbation, matching the ES-at-scale protocol.

Run on the GPU box:  python3 es-model-server.py <model-name> <port>
"""
import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer

import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

MODEL_NAME = sys.argv[1] if len(sys.argv) > 1 else "Qwen/Qwen2.5-0.5B-Instruct"
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 7779

device = "cuda" if torch.cuda.is_available() else "mps" if torch.backends.mps.is_available() else "cpu"
tokenizer = AutoTokenizer.from_pretrained(MODEL_NAME)
model = AutoModelForCausalLM.from_pretrained(MODEL_NAME, torch_dtype=torch.bfloat16).to(device)
model.eval()
LOCK = threading.Lock()  # the ES loop swaps weights; serialize generation


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/v1/chat/completions":
            self.send_error(404)
            return
        length = int(self.headers.get("Content-Length", 0))
        data = json.loads(self.rfile.read(length))
        messages = data.get("messages", [])
        with LOCK, torch.no_grad():
            prompt = tokenizer.apply_chat_template(
                messages, tokenize=False, add_generation_prompt=True,
                enable_thinking=False,
            )
            inputs = tokenizer(prompt, return_tensors="pt").to(device)
            out = model.generate(**inputs, max_new_tokens=256, do_sample=False,
                                 temperature=None, top_p=None, top_k=None)
            text = tokenizer.decode(out[0][inputs["input_ids"].shape[1]:], skip_special_tokens=True)
        n_in, n_out = inputs["input_ids"].shape[1], out.shape[1] - inputs["input_ids"].shape[1]
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


def perturb(sigma, seed):
    """Layer-level in-place Gaussian perturbation, restorable by seed.

    Mirrors the ES-at-scale Algorithm 2: noise is regenerated from the
    archived seed to restore exact parameters after evaluation.
    """
    with LOCK, torch.no_grad():
        generator = torch.Generator(device="cpu").manual_seed(seed)
        saved = {}
        for name, layer in model.named_modules():
            params = [(n, p) for n, p in layer.named_parameters(recurse=False)]
            if not params:
                continue
            for pname, p in params:
                noise = torch.randn(p.shape, generator=generator).to(p.dtype).to(p.device)
                p.add_(noise * sigma)
                saved[(name, pname)] = seed  # regeneration is deterministic per (seed, order)
        return saved


def restore(sigma, seed):
    with LOCK, torch.no_grad():
        generator = torch.Generator(device="cpu").manual_seed(seed)
        for name, layer in model.named_modules():
            params = [(n, p) for n, p in layer.named_parameters(recurse=False)]
            if not params:
                continue
            for pname, p in params:
                noise = torch.randn(p.shape, generator=generator).to(p.dtype).to(p.device)
                p.sub_(noise * sigma)


if __name__ == "__main__":
    server = HTTPServer(("127.0.0.1", PORT), Handler)
    print(f"es-model-server on 127.0.0.1:{PORT}, model={MODEL_NAME}, device={device}", file=sys.stderr)
    server.serve_forever()
