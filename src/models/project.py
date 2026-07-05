from dataclasses import dataclass, field
from typing import Optional
import json
import uuid


@dataclass
class Point:
    x: float
    y: float
    index: int
    label: Optional[str] = None
    category: Optional[str] = None


@dataclass
class Measurement:
    id: str
    type: str                            # "line" | "polyline" | "polygon"
    label: str                           # fragment name, e.g. "Frag-01"
    points: list[tuple[float, float]]    # image-space pixel coords [(x, y), ...]
    value: float                         # length (unit) or area (unit²)
    unit: str                            # "cm" or "m"
    species: str = ""                    # genus / species name
    auto_width: float = 0.0             # oriented/bbox width in real units
    auto_height: float = 0.0            # oriented/bbox height in real units
    area: float = 0.0                   # polygon area in real units²
    perimeter_len: float = 0.0          # polygon perimeter in real units
    angle: float = 0.0                  # tilt of the height axis, degrees from horizontal

    @staticmethod
    def new(mtype: str, label: str,
            points: list[tuple[float, float]],
            value: float, unit: str,
            species: str = "",
            auto_width: float = 0.0,
            auto_height: float = 0.0,
            area: float = 0.0,
            perimeter_len: float = 0.0,
            angle: float = 0.0) -> "Measurement":
        return Measurement(id=str(uuid.uuid4()), type=mtype, label=label,
                           points=points, value=value, unit=unit,
                           species=species,
                           auto_width=auto_width, auto_height=auto_height,
                           area=area, perimeter_len=perimeter_len, angle=angle)


@dataclass
class ImageAnnotation:
    image_path: str
    points: list[Point] = field(default_factory=list)
    image_width: int = 0
    image_height: int = 0
    scale_factor: float = 1.0   # pixels per scale_unit; 1.0 = not calibrated
    scale_unit: str = "cm"      # "cm" or "m"
    measurements: list[Measurement] = field(default_factory=list)

    def labeled_count(self) -> int:
        return sum(1 for p in self.points if p.label is not None)

    def is_complete(self) -> bool:
        return bool(self.points) and self.labeled_count() == len(self.points)

    def coverage_stats(self) -> dict:
        labeled = [p for p in self.points if p.label is not None]
        if not labeled:
            return {}
        total = len(labeled)
        stats: dict[str, int] = {}
        for p in labeled:
            key = p.label or ""
            stats[key] = stats.get(key, 0) + 1
        return {k: round(v / total * 100, 2) for k, v in stats.items()}


@dataclass
class Station:
    name: str
    depth_m: Optional[float] = None
    date: Optional[str] = None      # ISO-8601: "2024-03-15"
    gps_lat: Optional[float] = None
    gps_lon: Optional[float] = None
    notes: str = ""
    annotations: list[ImageAnnotation] = field(default_factory=list)

    def total_points(self) -> int:
        return sum(len(a.points) for a in self.annotations)

    def labeled_points(self) -> int:
        return sum(a.labeled_count() for a in self.annotations)

    def is_complete(self) -> bool:
        return bool(self.annotations) and all(
            a.is_complete() for a in self.annotations
        )


@dataclass
class Project:
    name: str
    point_count: int = 10
    point_distribution: str = "random"  # random | stratified | uniform
    border_exclusion: int = 0           # uniform pixel border to exclude
    border_rect: list | None = None     # [x_min, y_min, x_max, y_max] if set by click
    border_polygon: list | None = None  # [[x, y], ...] if set by polygon drawing
    coral_codes: dict = field(default_factory=dict)
    coral_groups: list = field(default_factory=list)  # [{"name": str, "codes": [str]}]
    species_list: list[str] = field(default_factory=list)  # known species/genus names
    stations: list[Station] = field(default_factory=list)
    save_path: Optional[str] = None

    @property
    def annotations(self) -> list[ImageAnnotation]:
        """Flat view across all stations — for statistics and export."""
        return [a for s in self.stations for a in s.annotations]

    def save(self, path: str) -> None:
        data = {
            "name": self.name,
            "point_count": self.point_count,
            "point_distribution": self.point_distribution,
            "border_exclusion": self.border_exclusion,
            "border_rect": self.border_rect,
            "border_polygon": self.border_polygon,
            "coral_codes": self.coral_codes,
            "coral_groups": self.coral_groups,
            "species_list": self.species_list,
            "stations": [
                {
                    "name": s.name,
                    "depth_m": s.depth_m,
                    "date": s.date,
                    "gps_lat": s.gps_lat,
                    "gps_lon": s.gps_lon,
                    "notes": s.notes,
                    "annotations": [
                        {
                            "image_path": a.image_path,
                            "image_width": a.image_width,
                            "image_height": a.image_height,
                            "scale_factor": a.scale_factor,
                            "scale_unit": a.scale_unit,
                            "points": [
                                {"x": p.x, "y": p.y, "index": p.index,
                                 "label": p.label, "category": p.category}
                                for p in a.points
                            ],
                            "measurements": [
                                {"id": m.id, "type": m.type, "label": m.label,
                                 "points": m.points, "value": m.value, "unit": m.unit,
                                 "species": m.species,
                                 "auto_width": m.auto_width, "auto_height": m.auto_height,
                                 "area": m.area, "perimeter_len": m.perimeter_len,
                                 "angle": m.angle}
                                for m in a.measurements
                            ],
                        }
                        for a in s.annotations
                    ],
                }
                for s in self.stations
            ],
        }
        with open(path, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2)
        self.save_path = path

    @classmethod
    def load(cls, path: str) -> "Project":
        with open(path, encoding="utf-8") as f:
            data = json.load(f)

        project = cls(
            name=data["name"],
            point_count=data["point_count"],
            point_distribution=data["point_distribution"],
            border_exclusion=data["border_exclusion"],
            border_rect=data.get("border_rect"),
            border_polygon=data.get("border_polygon"),
            coral_codes=data["coral_codes"],
            coral_groups=data.get("coral_groups", []),
            species_list=data.get("species_list", []),
        )

        def _load_annotation(a_data: dict) -> ImageAnnotation:
            ann = ImageAnnotation(
                image_path=a_data["image_path"],
                image_width=a_data["image_width"],
                image_height=a_data["image_height"],
                scale_factor=a_data.get("scale_factor", 1.0),
                scale_unit=a_data.get("scale_unit", "cm"),
            )
            for p_data in a_data["points"]:
                ann.points.append(Point(**p_data))
            for m_data in a_data.get("measurements", []):
                ann.measurements.append(Measurement(
                    id=m_data["id"],
                    type=m_data["type"],
                    label=m_data["label"],
                    points=[tuple(pt) for pt in m_data["points"]],
                    value=m_data["value"],
                    unit=m_data["unit"],
                    species=m_data.get("species", ""),
                    auto_width=m_data.get("auto_width", 0.0),
                    auto_height=m_data.get("auto_height", 0.0),
                    area=m_data.get("area", 0.0),
                    perimeter_len=m_data.get("perimeter_len", 0.0),
                    angle=m_data.get("angle", 0.0),
                ))
            return ann

        if "stations" in data:
            for s_data in data["stations"]:
                station = Station(
                    name=s_data["name"],
                    depth_m=s_data.get("depth_m"),
                    date=s_data.get("date"),
                    gps_lat=s_data.get("gps_lat"),
                    gps_lon=s_data.get("gps_lon"),
                    notes=s_data.get("notes", ""),
                )
                for a_data in s_data.get("annotations", []):
                    station.annotations.append(_load_annotation(a_data))
                project.stations.append(station)
        elif "annotations" in data:
            # Old flat format — auto-migrate to a single station
            station = Station(name="Station 1")
            for a_data in data["annotations"]:
                station.annotations.append(_load_annotation(a_data))
            project.stations.append(station)

        project.save_path = path
        return project
