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

import math
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ASSETS = Path(__file__).parent

# ── Palette ──────────────────────────────────────────────────────────────────
BG_OUTER   = (10,  45, 82,  255)   # deep ocean
BG_INNER   = (22,  82, 148, 255)   # mid ocean (radial gradient centre)
CORAL_MAIN = (224, 80, 40,  255)   # coral branch
CORAL_TIP  = (255, 140, 100, 255)  # polyp tips
CORAL_GLOW = (255, 180, 150, 100)  # soft glow ring on tips


# ── Drawing ───────────────────────────────────────────────────────────────────

def _radial_bg(draw: ImageDraw.ImageDraw, size: int) -> None:
    """Fake radial gradient with concentric circles."""
    steps = 32
    for i in range(steps, 0, -1):
        t = i / steps
        r = int(BG_OUTER[0] * t + BG_INNER[0] * (1 - t))
        g = int(BG_OUTER[1] * t + BG_INNER[1] * (1 - t))
        b = int(BG_OUTER[2] * t + BG_INNER[2] * (1 - t))
        radius = int(size * 0.5 * t)
        cx = size // 2
        cy = int(size * 0.55)
        draw.ellipse(
            [cx - radius, cy - radius, cx + radius, cy + radius],
            fill=(r, g, b, 255),
        )


def draw_icon(size: int) -> Image.Image:
    """Draw the coralX icon at the given pixel size."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    s = size / 512.0

    # ── Background rounded square ───────────────────────────────────────────
    corner = int(96 * s)
    draw.rounded_rectangle([0, 0, size - 1, size - 1], radius=corner, fill=BG_OUTER)
    _radial_bg(draw, size)

    # ── Helper: scale coordinates and widths ────────────────────────────────
    def p(x: float, y: float) -> tuple[int, int]:
        return int(x * s), int(y * s)

    def w(n: float) -> int:
        return max(1, int(n * s))

    # ── Coral branch structure ───────────────────────────────────────────────
    #
    #           ●   ●   ●   ●   ●
    #          /   /     \   \
    #         /   /       \   \
    #        ●───●         ●───●
    #         \                /
    #          ────────────────
    #               |
    #               |
    #               ●  (base)
    #
    branches = [
        # (x0, y0, x1, y1, thickness)
        (256, 445, 256, 295, 30),   # main stem
        (256, 335, 175, 222, 24),   # left arm
        (256, 335, 337, 222, 24),   # right arm
        (256, 295, 256, 188, 16),   # center spike
        (175, 222, 128, 148, 18),   # left-left twig
        (175, 222, 218, 144, 18),   # left-right twig
        (337, 222, 294, 144, 18),   # right-left twig
        (337, 222, 384, 148, 18),   # right-right twig
    ]

    for x0, y0, x1, y1, thickness in branches:
        draw.line([p(x0, y0), p(x1, y1)], fill=CORAL_MAIN, width=w(thickness))

    # ── Polyp tips ───────────────────────────────────────────────────────────
    tips = [(256, 186), (128, 143), (218, 139), (294, 139), (384, 143)]

    for cx, cy in tips:
        x, y = int(cx * s), int(cy * s)
        r_glow = int(22 * s)
        r_tip  = int(15 * s)
        r_dot  = int(6 * s)
        # Soft outer glow
        if r_glow > 0:
            draw.ellipse(
                [x - r_glow, y - r_glow, x + r_glow, y + r_glow],
                fill=CORAL_GLOW,
            )
        # Main tip circle
        draw.ellipse(
            [x - r_tip, y - r_tip, x + r_tip, y + r_tip],
            fill=CORAL_TIP,
        )
        # Bright centre dot
        if r_dot > 0:
            dot_color = (255, 210, 190, 255)
            draw.ellipse(
                [x - r_dot, y - r_dot, x + r_dot, y + r_dot],
                fill=dot_color,
            )

    # ── Sand base ────────────────────────────────────────────────────────────
    sand_y  = int(450 * s)
    sand_h  = int(62 * s)
    margin  = int(30 * s)
    if sand_h > 0:
        draw.ellipse(
            [margin, sand_y, size - margin, sand_y + sand_h],
            fill=(200, 170, 120, 80),
        )

    return img


# ── Export helpers ────────────────────────────────────────────────────────────

def save_pngs(sizes: list[int]) -> dict[int, Image.Image]:
    """Save individual PNGs and return the dict of images."""
    images: dict[int, Image.Image] = {}
    for size in sizes:
        img = draw_icon(size)
        path = ASSETS / f"icon_{size}.png"
        img.save(path)
        images[size] = img
        print(f"  {path.name}")
    return images


def save_ico(images: dict[int, Image.Image]) -> None:
    """Save Windows .ico with multiple embedded sizes."""
    ico_sizes = [16, 32, 48, 64, 128, 256]
    ico_images = [images[s].convert("RGBA") for s in ico_sizes if s in images]
    path = ASSETS / "icon.ico"
    ico_images[0].save(
        path,
        format="ICO",
        sizes=[(s, s) for s in ico_sizes],
        append_images=ico_images[1:],
    )
    print(f"  {path.name}")


def save_icns(images: dict[int, Image.Image]) -> None:
    """Save macOS .icns using iconutil (macOS only)."""
    if sys.platform != "darwin":
        print("  icon.icns — skipped (requires macOS + iconutil)")
        return

    iconset = ASSETS / "icon.iconset"
    iconset.mkdir(exist_ok=True)

    icns_map = {
        16: "icon_16x16.png",
        32: "icon_16x16@2x.png",
        32: "icon_32x32.png",  # noqa: F601
        64: "icon_32x32@2x.png",
        128: "icon_128x128.png",
        256: "icon_128x128@2x.png",
        256: "icon_256x256.png",  # noqa: F601
        512: "icon_256x256@2x.png",
        512: "icon_512x512.png",  # noqa: F601
    }

    for size, name in icns_map.items():
        if size in images:
            images[size].save(iconset / name)

    subprocess.run(
        ["iconutil", "-c", "icns", str(iconset), "-o", str(ASSETS / "icon.icns")],
        check=True,
    )
    shutil.rmtree(iconset)
    print("  icon.icns")


def save_master(img: Image.Image) -> None:
    path = ASSETS / "icon.png"
    img.save(path)
    print(f"  {path.name}  (master 512×512)")


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    """Generate all icon formats."""
    print("Generating coralX icons…")
    sizes = [16, 32, 48, 64, 128, 256, 512]
    images = save_pngs(sizes)
    save_master(images[512])
    save_ico(images)
    save_icns(images)
    print("Done.")


if __name__ == "__main__":
    main()
