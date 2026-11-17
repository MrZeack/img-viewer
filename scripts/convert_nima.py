"""Convierte NIMA MobileNet (estética, entrenado con AVA) de idealo a ONNX.

Uso (entorno aislado, no hace falta para compilar la app):
    uv venv -p 3.11 .venv
    uv pip install -p .venv/bin/python "tensorflow==2.15.0" "tf2onnx==1.16.1" "numpy<2" onnxruntime
    .venv/bin/python scripts/convert_nima.py <directorio de trabajo>

Escribe models/nima_mobilenet_aesthetic.onnx, verifica que ONNX Runtime coincide con Keras e
imprime los valores de referencia que usa el test de Rust (`scoring::nima`).
"""

import hashlib
import sys
import urllib.request
from pathlib import Path

import numpy as np

WEIGHTS_URL = (
    "https://github.com/idealo/image-quality-assessment/raw/master/"
    "models/MobileNet/weights_mobilenet_aesthetic_0.07.hdf5"
)
WEIGHTS_SHA256 = "e563ad91b3d47410e45f7238f07ab8f6abd1bd0c4b18a4b0af9c681a21a91cb2"
OUTPUT = Path(__file__).resolve().parent.parent / "models" / "nima_mobilenet_aesthetic.onnx"


def fetch_weights(workdir: Path) -> Path:
    path = workdir / "weights_mobilenet_aesthetic_0.07.hdf5"
    if not path.exists():
        urllib.request.urlretrieve(WEIGHTS_URL, path)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != WEIGHTS_SHA256:
        sys.exit(f"SHA256 inesperado: {digest}")
    return path


def build_model(weights: Path):
    import tensorflow as tf

    base = tf.keras.applications.MobileNet(
        input_shape=(224, 224, 3), weights=None, include_top=False, pooling="avg"
    )
    x = tf.keras.layers.Dropout(0.0)(base.output)
    x = tf.keras.layers.Dense(10, activation="softmax")(x)
    model = tf.keras.Model(base.inputs, x)
    model.load_weights(str(weights))
    return model


def pattern() -> np.ndarray:
    """Patrón determinista 224x224 RGB; el test de Rust genera el mismo."""
    y, x = np.mgrid[0:224, 0:224]
    rgb = np.stack([(x * 7 + y * 3) % 256, (x * y) % 256, (x + 2 * y) % 256], axis=-1)
    return rgb.astype(np.float32)


def preprocess(rgb: np.ndarray) -> np.ndarray:
    return (rgb / 127.5 - 1.0)[None, ...].astype(np.float32)


def mean_score(probs: np.ndarray) -> float:
    return float((probs[0] * np.arange(1, 11)).sum())


def main() -> None:
    import onnxruntime as ort
    import tensorflow as tf
    import tf2onnx

    workdir = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    model = build_model(fetch_weights(workdir))
    spec = (tf.TensorSpec((1, 224, 224, 3), tf.float32, name="input"),)
    OUTPUT.parent.mkdir(exist_ok=True)
    tf2onnx.convert.from_keras(model, input_signature=spec, opset=13, output_path=str(OUTPUT))

    session = ort.InferenceSession(str(OUTPUT))
    rng = np.random.default_rng(0)
    for sample in [rng.uniform(-1, 1, (1, 224, 224, 3)).astype(np.float32), preprocess(pattern())]:
        expected = model.predict(sample, verbose=0)
        actual = session.run(None, {"input": sample})[0]
        assert np.abs(expected - actual).max() < 1e-4, "ONNX no coincide con Keras"

    gray = np.full((224, 224, 3), 128, np.float32)
    for name, rgb in [("pattern", pattern()), ("gray128", gray)]:
        probs = session.run(None, {"input": preprocess(rgb)})[0]
        print(f"{name}: mean={mean_score(probs):.6f}")
    print(f"sha256(onnx)={hashlib.sha256(OUTPUT.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()
