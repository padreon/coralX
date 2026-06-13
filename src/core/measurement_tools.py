import math
from typing import Optional
import cv2
import numpy as np


def line_length(p1: tuple[float, float], p2: tuple[float, float],
                scale_factor: float, unit: str) -> float:
    """Straight-line distance between two image-space points in real units."""
    px_dist = math.hypot(p2[0] - p1[0], p2[1] - p1[1])
    return px_dist / scale_factor


def polyline_length(points: list[tuple[float, float]],
                    scale_factor: float, unit: str) -> float:
    """Sum of segment lengths along a polyline in real units."""
    total = 0.0
    for i in range(1, len(points)):
        total += math.hypot(points[i][0] - points[i - 1][0],
                            points[i][1] - points[i - 1][1])
    return total / scale_factor


def polygon_area(points: list[tuple[float, float]],
                 scale_factor: float, unit: str) -> float:
    """Polygon area via shoelace formula in real units squared."""
    n = len(points)
    if n < 3:
        return 0.0
    area_px2 = 0.0
    for i in range(n):
        x0, y0 = points[i]
        x1, y1 = points[(i + 1) % n]
        area_px2 += x0 * y1 - x1 * y0
    area_px2 = abs(area_px2) / 2.0
    return area_px2 / (scale_factor ** 2)


def magic_wand_select(image_path: str, seed_px: int, seed_py: int,
                      tolerance: int) -> Optional[list[tuple[float, float]]]:
    """
    Select pixels similar in color to the seed point using flood-fill.

    Uses FLOODFILL_FIXED_RANGE so every pixel is compared to the *seed*
    color (not its neighbors) — this gives true magic-wand behaviour where
    only pixels within `tolerance` of the clicked color are selected.

    Returns the largest contour as image-space (x, y) points, or None.
    """
    try:
        img = cv2.imread(image_path)
        if img is None:
            return None

        h, w = img.shape[:2]
        seed_px = max(0, min(w - 1, seed_px))
        seed_py = max(0, min(h - 1, seed_py))

        tol = max(1, int(tolerance))
        lo = (tol, tol, tol)
        hi = (tol, tol, tol)

        # floodFill requires mask 2px larger in each direction
        mask = np.zeros((h + 2, w + 2), dtype=np.uint8)
        flood_img = img.copy()

        # FLOODFILL_FIXED_RANGE: compare against seed pixel, not neighbors.
        # Without this flag the fill spreads along gradients and can cover
        # the entire image. (255 << 8) writes 255 into filled mask cells.
        flags = (cv2.FLOODFILL_FIXED_RANGE |
                 cv2.FLOODFILL_MASK_ONLY |
                 (255 << 8))

        cv2.floodFill(flood_img, mask, (seed_px, seed_py),
                      (0, 0, 0), lo, hi, flags)

        # Strip the 1-pixel padding required by floodFill
        region = mask[1:h + 1, 1:w + 1]

        contours, _ = cv2.findContours(
            region, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE
        )
        if not contours:
            return None

        largest = max(contours, key=cv2.contourArea)
        if cv2.contourArea(largest) < 9:
            return None

        # Simplify — 0.5 % of perimeter keeps shape faithful
        perimeter = cv2.arcLength(largest, True)
        epsilon = max(2.0, 0.005 * perimeter)
        approx = cv2.approxPolyDP(largest, epsilon, True)

        if len(approx) < 3:
            return None

        return [(float(pt[0][0]), float(pt[0][1])) for pt in approx]

    except Exception:
        return None
