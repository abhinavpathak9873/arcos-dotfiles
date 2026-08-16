#!/usr/bin/env python3
"""Generate speech locally with ArcOS's pinned Kokoro 82M model."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys

import numpy as np
import soundfile as sf
import torch
from kokoro import KModel, KPipeline


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="kokoro-tts",
        description="Offline Kokoro 82M speech synthesis with ArcOS's bundled voice.",
    )
    parser.add_argument("text", nargs="*", help="text to speak; stdin is used when omitted")
    parser.add_argument("-o", "--output", default="kokoro.wav", help="output WAV path")
    parser.add_argument("--play", action="store_true", help="play the generated WAV with PipeWire")
    parser.add_argument("--speed", type=float, default=1.0, help="speech speed (default: 1.0)")
    parser.add_argument(
        "--device",
        choices=("auto", "cpu", "cuda"),
        default="auto",
        help="inference device (default: auto)",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    text = " ".join(args.text).strip() if args.text else sys.stdin.read().strip()
    if not text:
        print("kokoro-tts: provide text as arguments or on stdin", file=sys.stderr)
        return 2
    if args.speed <= 0:
        print("kokoro-tts: --speed must be greater than zero", file=sys.stderr)
        return 2

    data_dir = Path(os.environ["KOKORO_82M_DIR"])
    if args.device == "cuda" and not torch.cuda.is_available():
        print("kokoro-tts: CUDA was requested but is unavailable", file=sys.stderr)
        return 2
    device = "cuda" if args.device == "cuda" else (
        "cuda" if args.device == "auto" and torch.cuda.is_available() else "cpu"
    )

    model = KModel(
        repo_id="hexgrad/Kokoro-82M",
        config=str(data_dir / "config.json"),
        model=str(data_dir / "kokoro-v1_0.pth"),
    ).to(device).eval()
    pipeline = KPipeline(
        lang_code="a",
        repo_id="hexgrad/Kokoro-82M",
        model=model,
        device=device,
    )
    voice = str(data_dir / "voices" / "af_heart.pt")
    chunks = [
        result.audio.detach().cpu().numpy()
        for result in pipeline(text, voice=voice, speed=args.speed)
        if result.audio is not None
    ]
    if not chunks:
        print("kokoro-tts: the input produced no audio", file=sys.stderr)
        return 1

    output = Path(args.output).expanduser().resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    audio = np.concatenate(chunks)
    sf.write(output, audio, 24_000, subtype="PCM_16")
    print(output)
    if args.play:
        subprocess.run(["pw-play", str(output)], check=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
