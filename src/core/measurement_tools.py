import math
from typing import Optional
import cv2
import numpy as np


# --- Magic-wand tuning -------------------------------------------------------
# Underwater lighting shades the same coral very differently, so the Lightness
# (L) channel is allowed to drift further than the chroma (a, b) channels: the
# same material at a different brightness should still be captured, while a
# genuinely different colour should not. Tolerance is the user's slider value.
MW_L_WEIGHT = 2.0          # L tolerance multiplier (permissive on brightness)
MW_AB_WEIGHT = 1.0         # a/b tolerance multiplier (strict on hue)
MW_MEDIAN_KSIZE = 5        # median blur to kill coral speckle; odd, 0 disables
MW_DENOISE_DIAMETER = 5    # bilateral-filter diameter; 0 disables denoising
MW_CLOSE_FRAC = 0.006      # close-kernel size as fraction of min(h, w)
MW_CLOSE_MAX = 25          # clamp the adaptive close kernel (px)
MW_MIN_SPECK_FRAC = 0.002  # drop foreground blobs smaller than this × fill area
MW_HOLE_FILL_FRAC = 0.05   # fill interior holes smaller than this × fill area
MW_MIN_AREA_ABS = 16       # absolute pixel floor for speck/hole removal
MW_FOCUS_GATE = True       # block the fill from bleeding into blurry background
MW_FOCUS_WIN_FRAC = 0.012  # local-sharpness window as fraction of min(h, w)

# A fragment only gets a TILTED measurement box when it genuinely leans like a
# stick: it must be BOTH clearly elongated (long/short ≥ ORIENT_MIN_ASPECT) AND
# leaning by more than ORIENT_MIN_ANGLE degrees. Anything else — chunky shapes,
# or long shapes that stand roughly upright — uses the upright axis-aligned box.
# Default is upright; tilt is the deliberate exception for leaning sticks.
ORIENT_MIN_ASPECT = 2.2    # long/short ratio below this = not stick-like
ORIENT_MIN_ANGLE = 8.0     # lean (deg from vertical/horizontal) below this snaps upright


# Cache the expensive preprocessing (imread + median + bilateral + LAB convert,
# plus the focus map) so repeated union/subtract clicks on the SAME image reuse
# it instead of recomputing from scratch every click. Holds one image at a time.
_FLOOD_CACHE: dict = {"key": None, "lab": None, "focus": None}


def _focus_region(img_bgr: np.ndarray) -> Optional[np.ndarray]:
    """Boolean (h, w) map of in-focus pixels, or None if gating doesn't apply.

    Out-of-focus background is blurred, so it carries little high-frequency
    detail; in-focus coral is full of crisp texture and edges. We measure local
    sharpness as the energy of the Laplacian in a small window, Otsu-threshold
    it into sharp/blurry, then morphologically CLOSE the sharp map: smooth coral
    interiors get bridged (they're ringed by sharp coral texture) while blurry
    background — which has no sharp anchors — stays empty. The complement of the
    result is used as a flood barrier so the fill can't cross into the blur.

    Returns None when the split is degenerate (almost all / almost none of the
    image is "in focus"), so a uniformly-focused photo is never wrongly gated.
    """
    h, w = img_bgr.shape[:2]
    gray = cv2.cvtColor(img_bgr, cv2.COLOR_BGR2GRAY)

    lap = cv2.Laplacian(gray, cv2.CV_32F, ksize=3)
    win = max(9, int(round(min(h, w) * MW_FOCUS_WIN_FRAC)) | 1)  # odd
    energy = cv2.boxFilter(lap * lap, -1, (win, win))

    # Compress dynamic range (sqrt) before Otsu so a few very sharp pixels don't
    # dominate the normalisation.
    e8 = cv2.normalize(np.sqrt(energy), None, 0, 255,
                       cv2.NORM_MINMAX).astype(np.uint8)
    _, sharp = cv2.threshold(e8, 0, 255, cv2.THRESH_BINARY + cv2.THRESH_OTSU)

    # Close to merge sharp texture into solid in-focus regions and fill smooth
    # coral interiors; kernel ~ the sharpness window.
    k = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (win, win))
    sharp = cv2.morphologyEx(sharp, cv2.MORPH_CLOSE, k)

    focus = sharp > 0
    frac = float(focus.mean())
    if frac < 0.05 or frac > 0.95:
        return None  # not a clear focus/blur split — don't gate

    # Dilate a little so genuinely in-focus coral right at the edge isn't clipped.
    focus = cv2.dilate(focus.astype(np.uint8), k) > 0
    return focus


def _prepare(image_path: str) -> tuple[Optional[np.ndarray], Optional[np.ndarray]]:
    """Return (speckle-suppressed LAB image, in-focus mask), cached per file.

    The cache key includes the file's mtime + the tuning constants, so edits to
    the file or the constants invalidate it automatically.
    """
    try:
        import os
        mtime = os.path.getmtime(image_path)
    except OSError:
        mtime = 0.0
    key = (image_path, mtime, MW_MEDIAN_KSIZE, MW_DENOISE_DIAMETER,
           MW_FOCUS_GATE, MW_FOCUS_WIN_FRAC)
    if _FLOOD_CACHE["key"] == key:
        return _FLOOD_CACHE["lab"], _FLOOD_CACHE["focus"]

    img = cv2.imread(image_path)
    if img is None:
        return None, None

    # Focus map is measured on the ORIGINAL image, before denoising removes the
    # very high-frequency detail that distinguishes sharp from blurry.
    focus = _focus_region(img) if MW_FOCUS_GATE else None

    # Two-stage texture suppression so coral speckle doesn't punch holes in the
    # fill: median blur kills salt-and-pepper specks, bilateral edge-preserves.
    if MW_MEDIAN_KSIZE > 0:
        img = cv2.medianBlur(img, MW_MEDIAN_KSIZE)
    if MW_DENOISE_DIAMETER > 0:
        img = cv2.bilateralFilter(img, MW_DENOISE_DIAMETER, 50, 50)
    lab = cv2.cvtColor(img, cv2.COLOR_BGR2LAB)

    _FLOOD_CACHE["key"] = key
    _FLOOD_CACHE["lab"] = lab
    _FLOOD_CACHE["focus"] = focus
    return lab, focus


def _fill_small_holes(region: np.ndarray, max_hole_area: float) -> np.ndarray:
    """Fill enclosed background holes in a 0/255 mask up to max_hole_area px.

    A hole is a background component that does NOT touch the image border;
    border-connected background is the real outside and is left alone. Genuine
    large holes (above the threshold) are preserved, so donut shapes still work.
    Vectorised — no Python per-component loop.
    """
    h, w = region.shape
    inv = np.where(region == 255, np.uint8(0), np.uint8(255))
    num_b, labels_b, stats_b, _ = cv2.connectedComponentsWithStats(inv, 8)
    x = stats_b[:, cv2.CC_STAT_LEFT]
    y = stats_b[:, cv2.CC_STAT_TOP]
    bw = stats_b[:, cv2.CC_STAT_WIDTH]
    bh = stats_b[:, cv2.CC_STAT_HEIGHT]
    touches_border = (x == 0) | (y == 0) | (x + bw == w) | (y + bh == h)
    fill = (~touches_border) & (stats_b[:, cv2.CC_STAT_AREA] < max_hole_area)
    fill[0] = False
    region[fill[labels_b]] = 255
    return region


def _clean_fill_region(region: np.ndarray) -> np.ndarray:
    """Clean a binary (0/255) magic-wand fill region in-place-safe fashion.

    Two area-driven passes (far more reliable than a fixed morphology kernel,
    which can only touch ~1px features):

    1. Remove isolated foreground specks smaller than
       max(MW_MIN_AREA_ABS, MW_MIN_SPECK_FRAC × fill_area).
    2. Fill interior background holes smaller than
       max(MW_MIN_AREA_ABS, MW_HOLE_FILL_FRAC × fill_area).

    Genuine large holes (e.g. a real gap in a plate) exceed the threshold and
    are preserved, so donut shapes and the subtract workflow still work.
    """
    fg_area = int(np.count_nonzero(region))
    if fg_area == 0:
        return region

    # 1) Remove small foreground specks. Vectorised: build a per-label keep
    #    table, then index it by the label image in one pass (no Python loop,
    #    which matters when speckle yields thousands of components).
    speck_thr = max(MW_MIN_AREA_ABS, fg_area * MW_MIN_SPECK_FRAC)
    num, labels, stats, _ = cv2.connectedComponentsWithStats(region, 8)
    keep = stats[:, cv2.CC_STAT_AREA] >= speck_thr
    keep[0] = False  # label 0 is background
    region = np.where(keep[labels], np.uint8(255), np.uint8(0))

    # 2) Fill small interior holes.
    region = _fill_small_holes(region, max(MW_MIN_AREA_ABS,
                                           fg_area * MW_HOLE_FILL_FRAC))
    return region


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


def mask_area(mask: np.ndarray, scale_factor: float) -> float:
    """Pixel-accurate area from a floodFill mask (h+2, w+2) in real units²."""
    region = mask[1:mask.shape[0] - 1, 1:mask.shape[1] - 1]
    return float(np.count_nonzero(region)) / (scale_factor ** 2)


def bounding_box(points: list[tuple[float, float]],
                 scale_factor: float) -> tuple[float, float]:
    """(width_real, height_real) from axis-aligned bounding box of points."""
    if not points:
        return 0.0, 0.0
    xs = [p[0] for p in points]
    ys = [p[1] for p in points]
    return (max(xs) - min(xs)) / scale_factor, (max(ys) - min(ys)) / scale_factor


def _rotated_box(pts: np.ndarray) -> tuple[float, float, float]:
    """minAreaRect of the points → (long_side, short_side, height_angle_deg).

    height_angle is the lean of the LONG (height) axis from horizontal,
    normalised to (-90, 90].
    """
    (_cx, _cy), (rw, rh), angle = cv2.minAreaRect(pts)
    long_side, short_side = (rw, rh) if rw >= rh else (rh, rw)
    height_angle = angle if rh >= rw else angle + 90.0
    height_angle = ((height_angle + 90.0) % 180.0) - 90.0  # → (-90, 90]
    return long_side, short_side, height_angle


def _should_tilt(long_side: float, short_side: float, height_angle: float) -> bool:
    """True only for a stick-like fragment that genuinely leans.

    Requires BOTH a high elongation (long/short ≥ ORIENT_MIN_ASPECT) AND a lean
    bigger than ORIENT_MIN_ANGLE away from upright/flat. The angle is measured
    against the nearest axis (vertical OR horizontal), so a stick standing
    upright and a stick lying flat both count as "not leaning".
    """
    if short_side <= 1e-6 or long_side / short_side < ORIENT_MIN_ASPECT:
        return False
    # Distance of the height axis from the nearest of vertical (±90) / horizontal (0).
    lean = min(abs(height_angle), abs(abs(height_angle) - 90.0))
    return lean >= ORIENT_MIN_ANGLE


def oriented_extent(points: list[tuple[float, float]],
                    scale_factor: float) -> tuple[float, float, float]:
    """(width_real, height_real, angle_deg) from the minimum-area ROTATED box.

    A branching coral photographed at a tilt leans across the image, so its true
    height runs along its own principal axis — NOT the image's vertical (Y) axis.
    cv2.minAreaRect fits the tightest *rotated* rectangle to the shape; we report
    its longer side as HEIGHT (along the lean) and its shorter side as WIDTH
    (perpendicular). angle_deg is the lean of the height axis from horizontal.

    Only a stick-like fragment that genuinely leans gets a tilted box; anything
    else falls back to the upright axis-aligned box with angle 0. See
    ORIENT_MIN_ASPECT / ORIENT_MIN_ANGLE and _should_tilt().
    """
    if len(points) < 2:
        return 0.0, 0.0, 0.0
    pts = np.array(points, dtype=np.float32)
    long_side, short_side, height_angle = _rotated_box(pts)

    # Not a leaning stick → upright box, no tilt (the default).
    if not _should_tilt(long_side, short_side, height_angle):
        xs, ys = pts[:, 0], pts[:, 1]
        w_real = (float(xs.max()) - float(xs.min())) / scale_factor
        h_real = (float(ys.max()) - float(ys.min())) / scale_factor
        return w_real, h_real, 0.0

    return short_side / scale_factor, long_side / scale_factor, float(height_angle)


def oriented_axes(points: list[tuple[float, float]]) -> Optional[dict]:
    """Geometry (image-space px) for drawing the tilt-aligned box and H/W axes.

    Returns a dict with:
      'corners'      : 4 (x, y) corners of the rotated box,
      'height_line'  : ((x1, y1), (x2, y2)) centre line along the LONG axis,
      'width_line'   : ((x1, y1), (x2, y2)) centre line along the SHORT axis,
    or None when there aren't enough points.
    """
    if len(points) < 2:
        return None
    pts = np.array(points, dtype=np.float32)
    long_side, short_side, height_angle = _rotated_box(pts)

    # Not a leaning stick → draw the upright axis-aligned box (matches the
    # angle-0 fallback in oriented_extent so the figure and the number agree).
    if not _should_tilt(long_side, short_side, height_angle):
        x0, y0 = float(pts[:, 0].min()), float(pts[:, 1].min())
        x1, y1 = float(pts[:, 0].max()), float(pts[:, 1].max())
        cx, cy = (x0 + x1) / 2.0, (y0 + y1) / 2.0
        return {
            "corners": [(x0, y0), (x1, y0), (x1, y1), (x0, y1)],
            "height_line": ((cx, y0), (cx, y1)),
            "width_line": ((x0, cy), (x1, cy)),
        }

    box = cv2.boxPoints(cv2.minAreaRect(pts))  # 4×2, ordered around the rect
    c = [(float(x), float(y)) for x, y in box]

    def mid(a, b):
        return ((c[a][0] + c[b][0]) / 2.0, (c[a][1] + c[b][1]) / 2.0)

    len_a = math.hypot(c[1][0] - c[0][0], c[1][1] - c[0][1])  # edge c0-c1
    len_b = math.hypot(c[2][0] - c[1][0], c[2][1] - c[1][1])  # edge c1-c2
    if len_a >= len_b:
        # c0-c1 / c2-c3 are the long edges → height spans the short-edge midpoints
        height_line = (mid(1, 2), mid(3, 0))
        width_line = (mid(0, 1), mid(2, 3))
    else:
        height_line = (mid(0, 1), mid(2, 3))
        width_line = (mid(1, 2), mid(3, 0))
    return {"corners": c, "height_line": height_line, "width_line": width_line}


def contour_perimeter(points: list[tuple[float, float]],
                      scale_factor: float) -> float:
    """Perimeter of a closed polygon in real units."""
    n = len(points)
    if n < 2:
        return 0.0
    total = sum(
        math.hypot(points[(i + 1) % n][0] - points[i][0],
                   points[(i + 1) % n][1] - points[i][1])
        for i in range(n)
    )
    return total / scale_factor


def magic_wand_subtract(
    image_path: str,
    seed_px: int,
    seed_py: int,
    tolerance: int,
    existing_mask: np.ndarray,
) -> Optional[np.ndarray]:
    """
    Right-click subtract: remove only the natural image feature under the seed
    (e.g. one hole in a plate), leaving spatially-separate regions of similar
    color (e.g. a second hole) intact.

    Why a single fixed tolerance fails the "2 holes in a plate" case
    ----------------------------------------------------------------
    The selection was made with a deliberately high tolerance so it swallowed
    the whole fragment — plate AND both holes. Inside that selection, hole 1 and
    hole 2 are connected by a continuous bridge of selected plate pixels. With a
    fixed subtract tolerance, a fill seeded in hole 1 only has to find a path of
    within-tolerance pixels to reach hole 2 — and because every selected pixel
    already lies within the *broad* selection tolerance of the others, that path
    exists. Lowering the tolerance to a fixed fraction (//3, sqrt, …) just shifts
    where it breaks: if the fraction is below the hole→plate color step it works,
    if not it still bleeds. There is no single fraction that is right for every
    image.

    The fix: grow tolerance until the boundary "breaks through"
    ----------------------------------------------------------
    Instead of guessing one tolerance, we sweep it from tight to broad and watch
    the filled area. A hole is a roughly uniform patch bounded by a real (even if
    thin) color step at the plate edge. While the tolerance stays below that step
    the fill is confined to the hole and its area grows gently. The instant the
    tolerance crosses the step, the fill spills out into the plate (and onward to
    hole 2) and the area jumps by a large multiple in a single step. We detect
    that jump and keep the mask from *just before* it — i.e. the clicked feature
    fully captured, but stopped at its natural boundary. This is what makes
    hole 1 removable while hole 2 survives: hole 2 only enters the fill *after*
    the break-through that we reject.

    The sweep is also constrained to currently-selected pixels (non-selected
    pixels act as flood barriers), so the subtract can never grow outside the
    selection and the area numbers reflect only meaningful growth.

    Edge cases where this still under/over-removes:
    - If the hole→plate color step is genuinely smaller than the within-hole
      color variation, no tolerance separates them; the first non-empty fill
      already includes the plate, no jump is seen, and the whole body is removed.
    - A feature that is a large fraction of the selection, or one with a soft
      gradient edge (no distinct step), produces a gradual rather than sudden
      area increase and may be partially removed.
    """
    try:
        img = cv2.imread(image_path)
        if img is None:
            return None

        h, w = img.shape[:2]
        if existing_mask.shape != (h + 2, w + 2):
            return None

        seed_px = max(0, min(w - 1, seed_px))
        seed_py = max(0, min(h - 1, seed_py))

        # Seed must lie inside the current selection — nothing to subtract
        # otherwise. (mask is padded by 1px, hence the +1 offset.)
        if existing_mask[seed_py + 1, seed_px + 1] != 255:
            return existing_mask.copy()

        flood_img = cv2.cvtColor(img, cv2.COLOR_BGR2LAB)

        # Barrier template: every pixel NOT currently selected is set to 1, which
        # floodFill treats as a wall. The subtract fill can therefore only travel
        # through already-selected pixels and never leaks outside the selection.
        selected = (existing_mask[1:h + 1, 1:w + 1] == 255)
        barrier_template = np.zeros((h + 2, w + 2), dtype=np.uint8)
        barrier_template[1:h + 1, 1:w + 1][~selected] = 1

        flags = (cv2.FLOODFILL_FIXED_RANGE | cv2.FLOODFILL_MASK_ONLY | (255 << 8))

        # Sweep tolerance from 1 up to the selection tolerance. We never need a
        # subtract tolerance broader than the one that made the selection.
        cap = max(1, int(tolerance))
        step = max(1, cap // 12)
        tol_values = list(range(1, cap + 1, step))
        if tol_values[-1] != cap:
            tol_values.append(cap)

        best_mask: Optional[np.ndarray] = None
        prev_mask: Optional[np.ndarray] = None
        prev_area = 0
        for tol in tol_values:
            m = barrier_template.copy()
            cv2.floodFill(flood_img, m, (seed_px, seed_py),
                          (0, 0, 0), (tol, tol, tol), (tol, tol, tol), flags)
            area = int(np.count_nonzero(m == 255))
            if area == 0:
                continue

            # Break-through detection: a sudden >3x jump (and a meaningful
            # absolute increase) means the fill just spilled past the feature's
            # natural edge into the connected body. Reject it; keep the prior mask.
            if prev_area > 0 and area > prev_area * 3 and area - prev_area > 50:
                best_mask = prev_mask
                break

            best_mask = m
            prev_mask = m
            prev_area = area

        if best_mask is None:
            return existing_mask.copy()

        result = existing_mask.copy()
        result[best_mask == 255] = 0
        return result

    except Exception:
        return None


def magic_wand_select(
    image_path: str,
    seed_px: int,
    seed_py: int,
    tolerance: int,
    smoothing: int = 0,
    existing_mask: Optional[np.ndarray] = None,
) -> tuple[Optional[list[tuple[float, float]]], Optional[np.ndarray]]:
    """
    Select pixels similar in color to the seed point using flood-fill.

    Uses FLOODFILL_FIXED_RANGE so every pixel is compared to the *seed*
    color (not its neighbors) — true magic-wand behaviour.

    Quality measures, in order of effect:
    - **Edge-preserving denoise** (bilateral filter) before the fill, so the
      fill follows real material boundaries instead of per-pixel sensor speckle.
      This alone removes most of the ragged, peppered edges.
    - **Channel-weighted tolerance** in LAB: the Lightness channel gets a wider
      band (MW_L_WEIGHT) and the chroma channels a tighter one (MW_AB_WEIGHT),
      so shadowed/highlit parts of the SAME coral are captured while
      differently-coloured neighbours are excluded.
    - **Morphological cleanup** of the freshly-filled region (close then open),
      which fills pinholes and drops isolated specks for a smooth boundary.
    - **Focus gating**: out-of-focus (blurry) background is detected and used as
      a flood barrier, so the fill can't bleed from the sharp coral into a
      similarly-coloured but defocused background.

    If `existing_mask` (h+2, w+2) is provided, the cleaned new fill is OR-ed
    into it so successive clicks expand the selection (union). Cleanup is
    applied only to the new fill so earlier regions are never eroded.

    Right-click reset is handled by the caller (pass existing_mask=None).

    Returns (contour_points, mask) or (None, None) on failure.
    mask has shape (h+2, w+2) matching the floodFill convention.
    """
    try:
        # Speckle-suppressed LAB image + in-focus map, cached so repeated clicks
        # on the same image don't re-read + re-filter from scratch.
        flood_img, focus = _prepare(image_path)
        if flood_img is None:
            return None, None

        h, w = flood_img.shape[:2]
        seed_px = max(0, min(w - 1, seed_px))
        seed_py = max(0, min(h - 1, seed_py))

        tol = max(1, int(tolerance))

        # Channel-weighted band: permissive on brightness (L), strict on hue (a,b).
        l_tol = max(1, int(round(tol * MW_L_WEIGHT)))
        ab_tol = max(1, int(round(tol * MW_AB_WEIGHT)))
        lo = (l_tol, ab_tol, ab_tol)
        hi = (l_tol, ab_tol, ab_tol)

        # FLOODFILL_FIXED_RANGE: compare against seed pixel, not neighbors.
        # (255 << 8) writes 255 into filled mask cells.
        flags = (cv2.FLOODFILL_FIXED_RANGE |
                 cv2.FLOODFILL_MASK_ONLY |
                 (255 << 8))

        # Fill into a FRESH mask so cleanup touches only the new region.
        new_mask = np.zeros((h + 2, w + 2), dtype=np.uint8)

        # Focus barrier: mark out-of-focus pixels as walls (1) so the fill stays
        # on the sharp coral. Skip gating if the user deliberately clicked a
        # blurry spot (its seed would otherwise be walled off and fill nothing).
        if focus is not None and focus[seed_py, seed_px]:
            new_mask[1:h + 1, 1:w + 1][~focus] = 1

        cv2.floodFill(flood_img, new_mask, (seed_px, seed_py),
                      (0, 0, 0), lo, hi, flags)

        # Keep only filled pixels (255); drop any focus-barrier markers (1) so
        # the region is a clean binary 0/255 mask for the cleanup passes.
        reg = np.where(new_mask[1:h + 1, 1:w + 1] == 255,
                       np.uint8(255), np.uint8(0))
        # Adaptive morphological close: kernel scaled to image size so it bridges
        # the speckle-sized holes the fill leaves behind (a fixed 3px kernel only
        # touches 1px gaps and is useless on real coral texture). Clamped so it
        # never grows large enough to merge separate objects.
        ksz = int(round(min(h, w) * MW_CLOSE_FRAC))
        ksz = max(3, min(ksz, MW_CLOSE_MAX))
        k = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (ksz, ksz))
        reg = cv2.morphologyEx(reg, cv2.MORPH_CLOSE, k)
        # Area-based cleanup: drop noise specks and fill remaining interior holes,
        # while preserving genuinely large holes (donut shapes still work).
        reg = _clean_fill_region(reg)
        new_mask[1:h + 1, 1:w + 1] = reg

        # Union the cleaned fill into the accumulated selection.
        if existing_mask is not None and existing_mask.shape == (h + 2, w + 2):
            mask = existing_mask.copy()
            mask[new_mask == 255] = 255
        else:
            mask = new_mask

        # Final hole-fill on the WHOLE union: per-click cleanup can't see holes
        # that only appear once regions are combined (seams between overlapping
        # fills, or a hole sealed by the union). Run it on the union region with
        # a threshold relative to the total selection so small holes vanish while
        # genuinely large gaps are kept.
        sel = mask[1:h + 1, 1:w + 1]
        sel_area = int(np.count_nonzero(sel))
        if sel_area > 0:
            mask[1:h + 1, 1:w + 1] = _fill_small_holes(
                sel.copy(), max(MW_MIN_AREA_ABS, sel_area * MW_HOLE_FILL_FRAC))

        # Strip the 1-pixel padding to get image-space region
        region = mask[1:h + 1, 1:w + 1]

        contours, _ = cv2.findContours(
            region, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE
        )
        if not contours:
            return None, mask

        largest = max(contours, key=cv2.contourArea)
        if cv2.contourArea(largest) < 9:
            return None, mask

        # smoothing 0 = follow every corner (epsilon ~0.5 px)
        # smoothing 100 = aggressive simplification (epsilon ~4% perimeter)
        perimeter = cv2.arcLength(largest, True)
        epsilon = 0.5 + (smoothing / 100.0) * 0.04 * perimeter
        approx = cv2.approxPolyDP(largest, epsilon, True)

        if len(approx) < 3:
            return None, mask

        pts = [(float(pt[0][0]), float(pt[0][1])) for pt in approx]
        return pts, mask

    except Exception:
        return None, None
