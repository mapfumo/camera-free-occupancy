#!/usr/bin/env bash
# Check the exported model against the STM32U585 with ST Edge AI Core.
#
#   ml/st_check.sh            analyze + validate on the 300 saved frames
#   ml/st_check.sh --all      validate on every labelled frame (~57,500)
#
# analyze   — what the model costs on the chip: flash, RAM, MACC per inference
# validate  — compiles the generated C model on this PC, runs it on real
#             frames, and compares its outputs with the PyTorch model's
#
# Run `ml/.venv/bin/python ml/train.py --final` first; it writes the ONNX
# model and the reference input/output pairs used here.
set -euo pipefail
cd "$(dirname "$0")/.."

STEDGEAI=${STEDGEAI:-/opt/ST/STEdgeAI/4.0/Utilities/linux/stedgeai}
MODEL=ml/results/occupancy_cnn.onnx
WS=ml/runs/st/ws
OUT=ml/runs/st/out
COMMON=(--model "$MODEL" --target stm32u5 --name occupancy --workspace "$WS" --output "$OUT")

if [[ "${1:-}" == "--all" ]]; then
    ml/.venv/bin/python - <<'EOF'
import sys; sys.path.insert(0, "ml")
import numpy as np, onnxruntime as ort
from dataset import load_all
d = load_all()
X = d["X"]
sess = ort.InferenceSession("ml/results/occupancy_cnn.onnx")
# the model takes one frame at a time (batch 1, as on the chip)
Y = np.concatenate([sess.run(None, {"input": X[i:i+1]})[0] for i in range(len(X))])
np.save("ml/runs/all_inputs.npy", X); np.save("ml/runs/all_outputs.npy", Y)
print(f"{len(X)} frames written for validation")
EOF
    IN=ml/runs/all_inputs.npy; REF=ml/runs/all_outputs.npy
else
    "$STEDGEAI" analyze "${COMMON[@]}" < /dev/null
    cp "$OUT/occupancy_analyze_report.txt" ml/results/stm32u5_analyze_report.txt
    IN=ml/results/val_inputs.npy; REF=ml/results/val_outputs.npy
fi

"$STEDGEAI" validate "${COMMON[@]}" --mode host --classifier \
    --valinput "$IN" --valoutput "$REF" < /dev/null
cp "$OUT/occupancy_validate_report.txt" \
   "ml/results/stm32u5_validate${1:+_all}_report.txt"
