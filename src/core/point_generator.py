import numpy as np
from src.models.project import Point


def generate_points(
    image_width: int,
    image_height: int,
    count: int,
    distribution: str = "random",
    border: int = 0,
    border_rect: list | None = None,
    border_polygon: list | None = None,
) -> list[Point]:
    """
    Generate points on an image.

    Args:
        image_width: Width of the image in pixels
        image_height: Height of the image in pixels
        count: Number of points to generate
        distribution: 'random', 'stratified', or 'uniform'
        border: Uniform pixel border to exclude (ignored when border_rect or border_polygon is set)
        border_rect: [x_min, y_min, x_max, y_max] from manual click; overrides border
        border_polygon: [[x, y], ...] polygon from manual drawing; overrides border_rect

    Returns:
        List of Point objects
    """
    if border_polygon and len(border_polygon) >= 3:
        coords = _generate_in_polygon(border_polygon, count, distribution)
        return [Point(x=float(x), y=float(y), index=i) for i, (x, y) in enumerate(coords)]

    if border_rect:
        x_min, y_min, x_max, y_max = border_rect
    else:
        x_min = border
        x_max = image_width - border
        y_min = border
        y_max = image_height - border

    if x_min >= x_max or y_min >= y_max:
        raise ValueError("Border exclusion too large for image size.")

    if distribution == "random":
        coords = _random_points(x_min, x_max, y_min, y_max, count)
    elif distribution == "stratified":
        coords = _stratified_points(x_min, x_max, y_min, y_max, count)
    elif distribution == "uniform":
        coords = _uniform_grid_points(x_min, x_max, y_min, y_max, count)
    else:
        raise ValueError(f"Unknown distribution: {distribution}")

    return [Point(x=float(x), y=float(y), index=i) for i, (x, y) in enumerate(coords)]


def _point_in_polygon(x: float, y: float, polygon: list) -> bool:
    """Ray-casting point-in-polygon test."""
    n = len(polygon)
    inside = False
    j = n - 1
    for i in range(n):
        xi, yi = float(polygon[i][0]), float(polygon[i][1])
        xj, yj = float(polygon[j][0]), float(polygon[j][1])
        if ((yi > y) != (yj > y)) and (x < (xj - xi) * (y - yi) / (yj - yi) + xi):
            inside = not inside
        j = i
    return inside


def _generate_in_polygon(polygon: list, count: int, distribution: str) -> list[tuple]:
    poly = np.array(polygon, dtype=float)
    x_min, y_min = float(poly[:, 0].min()), float(poly[:, 1].min())
    x_max, y_max = float(poly[:, 0].max()), float(poly[:, 1].max())

    if distribution == "uniform":
        density = max(count * 9, 900)
        cols = int(np.ceil(np.sqrt(density)))
        rows = int(np.ceil(density / cols))
        xs = np.linspace(x_min, x_max, cols)
        ys = np.linspace(y_min, y_max, rows)
        xv, yv = np.meshgrid(xs, ys)
        candidates = list(zip(xv.ravel().tolist(), yv.ravel().tolist()))
        inside = [(x, y) for x, y in candidates if _point_in_polygon(x, y, polygon)]
        if len(inside) >= count:
            step = max(1, len(inside) // count)
            return inside[::step][:count]
        return inside

    # random / stratified: rejection sampling
    coords: list[tuple] = []
    batch = max(count * 4, 200)
    max_iters = 50
    for _ in range(max_iters):
        if len(coords) >= count:
            break
        if distribution == "stratified":
            xs_b = np.random.uniform(x_min, x_max, batch)
            ys_b = np.random.uniform(y_min, y_max, batch)
        else:
            xs_b = np.random.uniform(x_min, x_max, batch)
            ys_b = np.random.uniform(y_min, y_max, batch)
        for x, y in zip(xs_b.tolist(), ys_b.tolist()):
            if len(coords) >= count:
                break
            if _point_in_polygon(x, y, polygon):
                coords.append((x, y))
    return coords[:count]


def _random_points(x_min, x_max, y_min, y_max, count):
    xs = np.random.uniform(x_min, x_max, count)
    ys = np.random.uniform(y_min, y_max, count)
    return list(zip(xs, ys))


def _stratified_points(x_min, x_max, y_min, y_max, count):
    """Stratified random sampling — divides image into grid cells, one point per cell."""
    cols = int(np.ceil(np.sqrt(count)))
    rows = int(np.ceil(count / cols))

    cell_w = (x_max - x_min) / cols
    cell_h = (y_max - y_min) / rows

    coords = []
    for row in range(rows):
        for col in range(cols):
            if len(coords) >= count:
                break
            x = x_min + col * cell_w + np.random.uniform(0, cell_w)
            y = y_min + row * cell_h + np.random.uniform(0, cell_h)
            x = min(x, x_max)
            y = min(y, y_max)
            coords.append((x, y))

    return coords[:count]


def _uniform_grid_points(x_min, x_max, y_min, y_max, count):
    """Uniform grid — evenly spaced points across the image."""
    cols = int(np.ceil(np.sqrt(count)))
    rows = int(np.ceil(count / cols))

    xs = np.linspace(x_min, x_max, cols)
    ys = np.linspace(y_min, y_max, rows)

    coords = [(x, y) for y in ys for x in xs]
    return coords[:count]
