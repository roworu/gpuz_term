"""assemble face textures into logo.png and icon_256.png"""

from pathlib import Path

from PIL import Image

HERE = Path(__file__).parent

FACES = {
    "top": (8.66, -5, 8.66, 5, 41.4, 78),
    "left": (8.66, 5, 0, 10, 41.4, 78),
    "right": (8.66, -5, 0, 10, 128, 128),
}


def render_png(size: int) -> Image.Image:

    k = size / 256
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    for name, (a, b, c, d, e, f) in FACES.items():
        tex = Image.open(HERE / "textures" / f"{name}.png").convert("RGBA")
        a, b, c, d, e, f = a * k, b * k, c * k, d * k, e * k, f * k
        det = a * d - b * c
        inv = (d / det, -c / det, (c * f - d * e) / det, -b / det, a / det, (b * e - a * f) / det)
        face = tex.transform((size, size), Image.AFFINE, inv, Image.NEAREST, fillcolor=(0, 0, 0, 0))
        out.alpha_composite(face)
    return out


def main() -> None:

    render_png(512).save(HERE / "logo.png")
    render_png(256).save(HERE / "icon_256.png")


if __name__ == "__main__":
    main()
