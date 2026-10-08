"""Draws Headroom's icons: a menu-bar template glyph and the 1024px app icon."""
from PIL import Image, ImageDraw

S = 4  # supersample for smooth edges


def tray(size: int, path: str):
    """Monochrome template icon (macOS tints it): two stacked meters, one partly filled."""
    W = size * S
    im = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    pad, h, gap, r = int(W * 0.08), int(W * 0.30), int(W * 0.12), int(W * 0.08)
    lw = max(S, int(W * 0.07))
    top = (W - (2 * h + gap)) // 2
    for i, fill in enumerate([0.45, 0.8]):
        y = top + i * (h + gap)
        d.rounded_rectangle([pad, y, W - pad, y + h], radius=r, outline=(0, 0, 0, 255), width=lw)
        inner = lw + int(W * 0.03)
        x2 = pad + inner + (W - 2 * pad - 2 * inner) * fill
        d.rounded_rectangle([pad + inner, y + inner, x2, y + h - inner], radius=max(1, r // 2), fill=(0, 0, 0, 255))
    im.resize((size, size), Image.LANCZOS).save(path)


def app_icon(path: str):
    W = 1024 * S
    im = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    m = int(W * 0.09)
    d.rounded_rectangle([m, m, W - m, W - m], radius=int(W * 0.2), fill=(28, 25, 23, 255))
    bars = [(0.42, (217, 119, 87)), (0.78, (240, 235, 228))]
    bx, bw, bh, gap = int(W * 0.22), int(W * 0.56), int(W * 0.14), int(W * 0.09)
    top = (W - (2 * bh + gap)) // 2
    for i, (fill, col) in enumerate(bars):
        y = top + i * (bh + gap)
        d.rounded_rectangle([bx, y, bx + bw, y + bh], radius=bh // 2, fill=(68, 62, 58, 255))
        d.rounded_rectangle([bx, y, bx + int(bw * fill), y + bh], radius=bh // 2, fill=col)
    im.resize((1024, 1024), Image.LANCZOS).save(path)


if __name__ == "__main__":
    import sys
    out = sys.argv[1]
    tray(22, f"{out}/tray.png")
    tray(44, f"{out}/tray@2x.png")
    app_icon(f"{out}/app-icon.png")
    print("ok")
