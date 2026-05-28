"""
Generate coralX icon in all required formats.

Usage:
    python assets/generate_icons.py

Outputs:
    assets/icon.png          512x512 master
    assets/icon.ico          Windows (16/32/48/64/128/256 px)
    assets/icon.icns         macOS   (requires macOS + iconutil)
    assets/icon_<N>.png      Individual sizes for Linux / debugging
"""
from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ASSETS = Path(__file__).parent

# ── Palette ───────────────────────────────────────────────────────────────────
BG_OUTER   = (10,  45,  82,  255)
BG_INNER   = (22,  82,  148, 255)
CORAL_MAIN = (224, 80,  40,  255)
CORAL_TIP  = (255, 140, 100, 255)
CORAL_GLOW = (255, 180, 150, 100)
CORAL_DOT  = (255, 210, 190, 255)
SAND       = (200, 170, 120, 80)

# Branch definitions: (x0, y0, x1, y1, thickness) in 512-px space
_BRANCHES = [
    (256, 445, 256, 295, 30),
    (256, 335, 175, 222, 24),
    (256, 335, 337, 222, 24),
    (256, 295, 256, 188, 16),
    (175, 222, 128, 148, 18),
    (175, 222, 218, 144, 18),
    (337, 222, 294, 144, 18),
    (337, 222, 384, 148, 18),
]

# Polyp tip centres in 512-px space
_TIPS = [(256, 186), (128, 143), (218, 139), (294, 139), (384, 143)]

# icns requires multiple files per logical size (normal + @2x)
_ICNS_ENTRIES: list[tuple[int, str]] = [
    (16,  "icon_16x16.png"),
    (32,  "icon_16x16@2x.png"),
    (32,  "icon_32x32.png"),
    (64,  "icon_32x32@2x.png"),
    (128, "icon_128x128.png"),
    (256, "icon_128x128@2x.png"),
    (256, "icon_256x256.png"),
    (512, "icon_256x256@2x.png"),
    (512, "icon_512x512.png"),
]


# ── Drawing ───────────────────────────────────────────────────────────────────

def _radial_bg(draw: ImageDraw.ImageDraw, size: int) -> None:
    """Fake radial gradient via concentric circles."""
    steps = 32
    cx, cy = size // 2, int(size * 0.55)
    for i in range(steps, 0, -1):
        t = i / steps
        color = tuple(
            int(BG_OUTER[c] * t + BG_INNER[c] * (1 - t)) for c in range(3)
        ) + (255,)
        r = int(size * 0.5 * t)
        draw.ellipse([cx - r, cy - r, cx + r, cy + r], fill=color)  # type: ignore[arg-type]


def _draw_tips(draw: ImageDraw.ImageDraw, scale: float) -> None:
    """Draw polyp tip circles."""
    for cx, cy in _TIPS:
        x, y = int(cx * scale), int(cy * scale)
        r_glow = int(22 * scale)
        r_tip  = int(15 * scale)
        r_dot  = int(6  * scale)
        if r_glow > 0:
            draw.ellipse([x - r_glow, y - r_glow, x + r_glow, y + r_glow], fill=CORAL_GLOW)
        draw.ellipse([x - r_tip, y - r_tip, x + r_tip, y + r_tip], fill=CORAL_TIP)
        if r_dot > 0:
            draw.ellipse([x - r_dot, y - r_dot, x + r_dot, y + r_dot], fill=CORAL_DOT)


def draw_icon(size: int) -> Image.Image:
    """Draw the coralX icon at the given pixel size."""
    img  = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    s    = size / 512.0

    draw.rounded_rectangle([0, 0, size - 1, size - 1], radius=int(96 * s), fill=BG_OUTER)
    _radial_bg(draw, size)

    for x0, y0, x1, y1, thickness in _BRANCHES:
        draw.line(
            [int(x0 * s), int(y0 * s), int(x1 * s), int(y1 * s)],
            fill=CORAL_MAIN,
            width=max(1, int(thickness * s)),
        )

    _draw_tips(draw, s)

    sand_y = int(450 * s)
    sand_h = int(62  * s)
    margin = int(30  * s)
    if sand_h > 0:
        draw.ellipse([margin, sand_y, size - margin, sand_y + sand_h], fill=SAND)

    return img


# ── Export helpers ────────────────────────────────────────────────────────────

def save_pngs(sizes: list[int]) -> dict[int, Image.Image]:
    """Render and save one PNG per size; return the image dict."""
    images: dict[int, Image.Image] = {}
    for size in sizes:
        img  = draw_icon(size)
        path = ASSETS / f"icon_{size}.png"
        img.save(path)
        images[size] = img
        print(f"  {path.name}")
    return images


def save_master(img: Image.Image) -> None:
    """Save the 512-px master PNG."""
    path = ASSETS / "icon.png"
    img.save(path)
    print(f"  {path.name}  (master 512×512)")


def save_ico(images: dict[int, Image.Image]) -> None:
    """Save Windows .ico with multiple embedded sizes."""
    ico_sizes  = [16, 32, 48, 64, 128, 256]
    ico_images = [images[s].convert("RGBA") for s in ico_sizes if s in images]
    path       = ASSETS / "icon.ico"
    ico_images[0].save(
        path,
        format="ICO",
        sizes=[(s, s) for s in ico_sizes],
        append_images=ico_images[1:],
    )
    print(f"  {path.name}")


def save_icns(images: dict[int, Image.Image]) -> None:
    """Save macOS .icns via iconutil (macOS only)."""
    if sys.platform != "darwin":
        print("  icon.icns — skipped (requires macOS + iconutil)")
        return

    iconset = ASSETS / "icon.iconset"
    iconset.mkdir(exist_ok=True)
    for size, filename in _ICNS_ENTRIES:
        if size in images:
            images[size].save(iconset / filename)

    subprocess.run(
        ["iconutil", "-c", "icns", str(iconset), "-o", str(ASSETS / "icon.icns")],
        check=True,
    )
    shutil.rmtree(iconset)
    print("  icon.icns")


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    """Generate all icon formats."""
    print("Generating coralX icons…")
    sizes  = [16, 32, 48, 64, 128, 256, 512]
    images = save_pngs(sizes)
    save_master(images[512])
    save_ico(images)
    save_icns(images)
    print("Done.")


if __name__ == "__main__":
    main()
