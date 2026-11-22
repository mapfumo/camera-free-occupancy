#!/usr/bin/env python3
"""Live 8x8 heatmap viewer for the CoworkOS sensors.

Reads the log output of `tof_range` or `thermal_probe` and draws it as a
picture that updates in real time. Its job is to make sensor aiming something
you *watch* rather than something you discover in a plot the next day.

The roadmap is specific about why this exists. The single biggest risk to the
project is that the sensor rig is mounted badly and every recorded frame
carries the same invisible defect, which does not show up until a model
underperforms for reasons nobody can diagnose. Watching a live heatmap while
positioning the rig is the cheapest defence against that, and it has to exist
*before* the rig is built to be any use at all.

Usage
-----

Pipe the firmware's output straight in::

    cd firmware
    cargo run --release --bin tof_range | python3 ../tools/heatmap_viewer.py

Same for the thermal sensor -- the sensor type is detected automatically::

    cargo run --release --bin thermal_probe | python3 ../tools/heatmap_viewer.py

Both sensors at once, paired frame by frame, side by side::

    cargo run --release --bin fused_stream | python3 ../tools/heatmap_viewer.py

Record frames to CSV at the same time. With ``fused_stream`` this writes one
row per pair -- raw distance, ToF status and temperature -- which is the
format Phase 2 collects in::

    cargo run --release --bin fused_stream \\
        | python3 ../tools/heatmap_viewer.py --record session.csv --label empty

The single-sensor binaries record the same way, one sensor per row::

    cargo run --release --bin tof_range \\
        | python3 ../tools/heatmap_viewer.py --record session.csv --label empty

Replay a previously captured log, with no hardware attached::

    python3 tools/heatmap_viewer.py --input saved_log.txt

Design notes
------------

*Parsing text output rather than a binary protocol.* The firmware already
prints human-readable grids for the benefit of anyone watching the terminal,
so consuming those costs no extra firmware code and keeps one source of truth.
It is not the right answer forever -- a binary frame format would be faster and
unambiguous -- but that belongs with the combined-stream work, not here.

*Colour maps are perceptually uniform, never rainbow.* `jet` and `turbo` are
the reflex choice for sensor data and they are actively misleading: their
lightness is not monotonic, so they invent bright and dark bands where the
underlying data is perfectly smooth, and the eye reads those bands as edges in
the scene. `viridis` and `inferno` rise steadily from dark to light, so what
looks like a boundary is one. They also survive colour-vision deficiency and
greyscale printing, which matters when these plots end up in the write-up.
"""

from __future__ import annotations

import argparse
import csv
import re
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Iterator, Optional, TextIO

GRID = 8
PIXELS = GRID * GRID

# ---------------------------------------------------------------------------
# Parsing
# ---------------------------------------------------------------------------
#
# The firmware prints, per frame, one header line followed by eight row lines:
#
#   [INFO ] --- frame 5 --- 47/64 valid, 137..261 mm, silicon 39 C (tof_range ...)
#   [INFO ]   -1 -1 -1 -1 241 235 -1 -1   |....%%..| (tof_range src/bin/...)
#
# defmt prepends a level tag and appends the source location, so both patterns
# match loosely in the middle of a line rather than anchoring to its start.

_FRAME_HEADER = re.compile(r"---\s*frame\s+(\d+)\s*---\s*(tof|status|thermal)?")

# Eight whitespace-separated integers immediately before the ASCII map, which
# is the part that makes a row line unambiguous. Matching on "eight integers"
# alone would also catch the header's "137..261".
_FRAME_ROW = re.compile(r"\s((?:-?\d+\s+){%d})\|" % GRID)

# defmt tags each line with the binary that emitted it, which is a more
# reliable sensor identifier than guessing from the header's wording.
_SOURCE = re.compile(r"\((tof_range|thermal_probe|fused_stream)\s")

# Metadata carried on fused_stream's header lines: the device clock on the
# tof block, the AMG8833 die temperature on the thermal block.
_DEVICE_MS = re.compile(r"\bt\s+(\d+)\s*ms")
_DIE = re.compile(r"\bdie\s+(-?\d+)")
# Mount angle from the on-board IMU, on the tof header; "tilt n/a" if absent.
_TILT = re.compile(r"\btilt\s+(-?[\d.]+)\s*deg,\s*roll\s+(-?[\d.]+)\s*deg")

# How far the mount may move from the session's first reading before the
# viewer says so. Single-sample noise is ~0.3 degrees; a knock is more.
TILT_ALARM_DEG = 1.0

# ULD target_status values that mean "this distance is a measurement". Must
# match `vl53l5cx::is_valid` in the firmware.
TOF_VALID_STATUS = frozenset({5, 9})


@dataclass
class Frame:
    """One complete 8x8 reading."""

    number: int
    """Frame counter from the firmware. Not contiguous -- the firmware prints
    every Nth frame to keep the terminal readable."""

    values: list[int]
    """64 values in row-major order: index = row * 8 + col. Both sensors use
    the same ordering, which is what lets the two grids be concatenated later
    without transposing either."""

    sensor: str
    """``tof_range`` or ``thermal_probe``; for ``fused_stream``, one of
    ``fused_stream:tof``, ``fused_stream:status`` or ``fused_stream:thermal``,
    naming which of the three blocks this is."""

    header: str = ""
    """The header line as printed, for metadata such as the device clock."""

    @property
    def rows(self) -> list[list[int]]:
        """The values as 8 rows of 8, for display."""
        return [self.values[r * GRID : (r + 1) * GRID] for r in range(GRID)]


class FrameParser:
    """Turns a stream of log lines into complete frames.

    A small state machine: a header line starts collecting, eight row lines
    complete a frame. Anything else is ignored, so boot messages, clock dumps
    and errors pass through harmlessly.

    Partial frames are discarded rather than padded. A frame interrupted by a
    reflash or a Ctrl-C is not a real observation, and half a grid quietly
    zero-filled is exactly the sort of thing that ends up in a training set.
    """

    def __init__(self) -> None:
        self._number: Optional[int] = None
        self._sensor: str = "unknown"
        self._kind: Optional[str] = None
        self._header: str = ""
        self._rows: list[list[int]] = []

    def feed(self, line: str) -> Optional[Frame]:
        """Consume one line. Returns a Frame only on the line that completes one."""
        source = _SOURCE.search(line)
        if source:
            self._sensor = source.group(1)

        header = _FRAME_HEADER.search(line)
        if header:
            # A new header abandons any half-collected frame. That happens
            # normally -- the firmware's own error paths print between frames.
            self._number = int(header.group(1))
            self._kind = header.group(2)
            self._header = line
            self._rows = []
            return None

        if self._number is None:
            return None  # Row lines before any header are not ours.

        row = _FRAME_ROW.search(line)
        if not row:
            return None

        values = [int(v) for v in row.group(1).split()]
        if len(values) != GRID:
            return None  # Defensive: the regex should already guarantee this.

        self._rows.append(values)
        if len(self._rows) < GRID:
            return None

        sensor = self._sensor
        if sensor == "fused_stream" and self._kind:
            sensor = f"fused_stream:{self._kind}"
        frame = Frame(
            number=self._number,
            values=[v for row_values in self._rows for v in row_values],
            sensor=sensor,
            header=self._header,
        )
        self._number = None
        self._rows = []
        return frame


def parse_stream(lines: Iterator[str]) -> Iterator[Frame]:
    """Yield complete frames from a stream of log lines."""
    parser = FrameParser()
    for line in lines:
        frame = parser.feed(line)
        if frame is not None:
            yield frame


@dataclass
class FusedFrame:
    """One paired observation from ``fused_stream``: both sensors, one moment.

    The ToF distances are kept **raw**, with the ULD status alongside, exactly
    as the firmware sends them. Deciding what an invalid zone means as model
    input is deliberately left for training time (see TODO.md); nothing here
    collapses it.
    """

    number: int
    device_ms: Optional[int]
    """Milliseconds since boot on the device's own clock."""
    distance: list[int]
    """Raw distance per zone, mm, world frame. Meaningless where invalid."""
    status: list[int]
    """ULD target_status per zone. See TOF_VALID_STATUS."""
    thermal: list[int]
    """Temperature per pixel, centidegrees C, world frame."""
    die: Optional[int]
    """AMG8833 thermistor, centidegrees C."""
    tilt_deg: Optional[float] = None
    """Optical axis below horizontal, degrees, one IMU sample. None if the
    firmware had no IMU reading."""
    roll_deg: Optional[float] = None
    """Rotation about the optical axis, degrees. Zero is arbitrary per mount."""

    def display_distance(self) -> Frame:
        """The ToF grid with invalid zones as -1, for drawing only."""
        values = [
            d if s in TOF_VALID_STATUS else -1
            for d, s in zip(self.distance, self.status)
        ]
        return Frame(number=self.number, values=values, sensor="tof_range")

    def display_thermal(self) -> Frame:
        return Frame(number=self.number, values=self.thermal, sensor="thermal_probe")


def assemble(frames: Iterator[Frame]) -> Iterator["Frame | FusedFrame"]:
    """Group fused_stream's three blocks into FusedFrames; pass others through.

    A pair is only yielded once all three blocks with one frame number have
    arrived. Anything incomplete -- the stream cut mid-frame, a block lost --
    is dropped, for the same reason the parser drops partial grids: a ToF
    frame without its thermal partner is not a paired observation.
    """
    pending: dict[str, Frame] = {}
    number: Optional[int] = None
    for frame in frames:
        if not frame.sensor.startswith("fused_stream:"):
            yield frame
            continue
        if frame.number != number:
            pending, number = {}, frame.number
        pending[frame.sensor.split(":", 1)[1]] = frame
        if pending.keys() >= {"tof", "status", "thermal"}:
            ms = _DEVICE_MS.search(pending["tof"].header)
            die = _DIE.search(pending["thermal"].header)
            tilt = _TILT.search(pending["tof"].header)
            yield FusedFrame(
                number=number,
                device_ms=int(ms.group(1)) if ms else None,
                distance=pending["tof"].values,
                status=pending["status"].values,
                thermal=pending["thermal"].values,
                die=int(die.group(1)) if die else None,
                tilt_deg=float(tilt.group(1)) if tilt else None,
                roll_deg=float(tilt.group(2)) if tilt else None,
            )
            pending, number = {}, None


# ---------------------------------------------------------------------------
# Sensor-specific presentation
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class SensorStyle:
    """How to draw one sensor's data.

    Kept as data rather than branching in the render path, so adding the fused
    two-sensor view later means adding an entry, not editing the renderer.
    """

    title: str
    units: str
    colormap: str
    invalid: Optional[int]
    """Sentinel meaning "no reading here", or None if every value is real.
    The ToF reports -1 for a zone with no valid target; the thermal sensor
    always returns a temperature."""

    scale: float
    """Divisor to convert the firmware's integers into display units."""

    decimals: int
    """Decimal places for the per-cell labels. Distances are whole
    millimetres; temperatures would lose the sensor's 0.25 °C resolution if
    rounded to whole degrees."""


STYLES = {
    # Distance: viridis runs dark->light with steadily increasing lightness, so
    # near and far are distinguishable even in greyscale.
    "tof_range": SensorStyle(
        title="VL53L5CX — distance",
        units="mm",
        colormap="viridis",
        invalid=-1,
        scale=1.0,
        decimals=0,
    ),
    # Temperature: inferno reads naturally as heat without being a rainbow --
    # its lightness rises monotonically, so hotter is genuinely brighter.
    "thermal_probe": SensorStyle(
        title="AMG8833 — temperature",
        units="°C",
        colormap="inferno",
        invalid=None,
        scale=100.0,  # centidegrees -> degrees
        decimals=1,
    ),
}

FALLBACK_STYLE = SensorStyle(
    title="sensor", units="", colormap="viridis", invalid=None, scale=1.0, decimals=0
)


# ---------------------------------------------------------------------------
# Live display
# ---------------------------------------------------------------------------


class HeatmapView:
    """A matplotlib window that redraws in place as frames arrive.

    Deliberately simple: one image, updated, with ``plt.pause`` to let the GUI
    process events between frames. A threaded producer/consumer would decouple
    parsing from drawing properly, but at a few frames per second the extra
    machinery buys nothing and costs readability.
    """

    def __init__(self, style: SensorStyle, annotate: bool = True, axes=None) -> None:
        # Imported here rather than at module scope so that --record works on a
        # machine with no display and no matplotlib installed.
        import matplotlib.pyplot as plt
        import numpy as np
        from matplotlib.colors import Normalize

        self._plt = plt
        self._np = np
        self._Normalize = Normalize
        self._style = style
        self._annotate = annotate

        plt.ion()
        if axes is None:
            self._figure, self._axes = plt.subplots(figsize=(6.5, 7.0))
        else:
            # Drawing into a panel of someone else's figure (the fused view).
            self._figure, self._axes = axes.figure, axes

        # Built once, not per frame. `set_bad` paints cells with no reading in
        # a neutral grey that is deliberately not a value anywhere on the ramp,
        # so "no target here" can never be misread as a distance.
        colormap = plt.get_cmap(style.colormap).copy()
        colormap.set_bad(color="#2b2b2b")

        # Start from a blank grid; real limits arrive with the first frame.
        # vmin/vmax are given explicitly so the initial norm is never
        # degenerate, which matplotlib handles poorly.
        blank = np.zeros((GRID, GRID))
        self._image = self._axes.imshow(
            blank, cmap=colormap, interpolation="nearest", vmin=0.0, vmax=1.0
        )

        self._colorbar = self._figure.colorbar(self._image, ax=self._axes, shrink=0.8)
        self._colorbar.set_label(style.units)

        # Recessive chrome: the data is the subject, the frame around it is not.
        self._axes.set_xticks(range(GRID))
        self._axes.set_yticks(range(GRID))
        self._axes.tick_params(length=0, labelsize=8, colors="#888888")
        for spine in self._axes.spines.values():
            spine.set_visible(False)

        # Zone borders, drawn faintly, so individual zones stay countable
        # without the grid competing with the data.
        self._axes.set_xticks([x - 0.5 for x in range(1, GRID)], minor=True)
        self._axes.set_yticks([y - 0.5 for y in range(1, GRID)], minor=True)
        self._axes.grid(which="minor", color="#ffffff", linewidth=0.5, alpha=0.25)

        self._labels: list = []
        if axes is None:
            self._figure.tight_layout()

    def update(self, frame: Frame, pause: bool = True) -> None:
        """Redraw with a new frame. ``pause=False`` leaves the GUI event pump
        to the caller, so a two-panel view pauses once per pair, not twice."""
        np = self._np
        style = self._style

        grid = np.array(frame.values, dtype=float).reshape(GRID, GRID)

        if style.invalid is not None:
            # Masked cells are drawn in the colormap's "bad" colour rather than
            # being given a number. A zone with no target is a real observation
            # -- "nothing within range this way" -- and painting it as though it
            # were a distance of -1 would put a false hard edge in the picture.
            grid = np.ma.masked_where(grid == style.invalid, grid)

        grid = grid / style.scale

        self._image.set_data(grid)

        # `count()` on a masked array excludes masked cells; a plain array has
        # no such method and every cell counts.
        valid = int(grid.count()) if np.ma.isMaskedArray(grid) else grid.size
        if valid == 0:
            # Every zone reported nothing — a real state, not an error. Leave
            # the previous scale rather than dividing by an empty range.
            self._axes.set_title(
                f"{style.title}\nframe {frame.number}   ·   no valid readings",
                fontsize=11,
            )
            if pause:
                self._plt.pause(0.001)
            return

        # Rescale to the frame's own range.
        #
        # Replacing the whole norm rather than calling `set_clim`, which is the
        # obvious choice and is subtly broken here: with a colourbar attached,
        # matplotlib 3.6's set_clim mangles vmin — asking for 815 produced 0.9,
        # and the plot renders with a silently wrong scale rather than an error.
        # Installing a fresh Normalize sidesteps it. Verified against 3.6.3.
        #
        # Auto-scaling per frame is the right choice for aiming: it keeps
        # maximum contrast on whatever is in view. It does mean colours are not
        # comparable between frames, which is a poor default for analysis but
        # the correct one for pointing a sensor at something.
        self._image.set_norm(
            self._Normalize(vmin=float(grid.min()), vmax=float(grid.max()))
        )
        self._colorbar.update_normal(self._image)
        self._axes.set_title(
            f"{style.title}\n"
            f"frame {frame.number}   ·   {valid}/{PIXELS} valid   ·   "
            f"{grid.min():.1f}–{grid.max():.1f} {style.units}",
            fontsize=11,
        )

        if self._annotate:
            self._draw_values(grid)

        # Hand control to the GUI briefly so the window stays responsive.
        if pause:
            self._plt.pause(0.001)

    def _draw_values(self, grid) -> None:
        """Print each cell's value on top of it.

        Text is white or black per cell depending on the background behind it,
        because a single ink colour is unreadable at one end of any ramp.
        """
        for label in self._labels:
            label.remove()
        self._labels = []

        low, high = self._image.get_clim()
        span = (high - low) or 1.0

        for row in range(GRID):
            for col in range(GRID):
                value = grid[row, col]
                if value is self._np.ma.masked:
                    continue
                # Bright cells get dark text and vice versa.
                shade = "#111111" if (value - low) / span > 0.55 else "#f0f0f0"
                self._labels.append(
                    self._axes.text(
                        col,
                        row,
                        f"{value:.{self._style.decimals}f}",
                        ha="center",
                        va="center",
                        fontsize=7,
                        color=shade,
                    )
                )

    def keep_open(self) -> None:
        """Hold the window open after the stream ends."""
        self._plt.ioff()
        self._plt.show()


class FusedView:
    """Both sensors side by side, one window, updated together.

    Same orientation in both panels -- the grids are already in the world
    frame -- so a person should appear in roughly the same place in each. The
    ToF's field is ~45 degrees against the thermal's ~60, so a ToF zone covers
    less of the room: expect the ToF patch to look larger, not identical.
    """

    def __init__(self, annotate: bool = True) -> None:
        import matplotlib.pyplot as plt

        plt.ion()
        self._plt = plt
        figure, (left, right) = plt.subplots(1, 2, figsize=(12.5, 6.5))
        self._tof = HeatmapView(STYLES["tof_range"], annotate, axes=left)
        self._thermal = HeatmapView(STYLES["thermal_probe"], annotate, axes=right)
        figure.tight_layout()

    def update(self, fused: FusedFrame) -> None:
        self._tof.update(fused.display_distance(), pause=False)
        self._thermal.update(fused.display_thermal(), pause=False)
        self._plt.pause(0.001)

    def keep_open(self) -> None:
        self._plt.ioff()
        self._plt.show()


# ---------------------------------------------------------------------------
# Recording
# ---------------------------------------------------------------------------


class CsvRecorder:
    """Append frames to a CSV, one row per frame.

    This is Phase 2's data collection in its simplest form. The label column is
    set once for a whole session and carried on every row, because the project
    plan's approach is to record one situation at a time rather than annotate
    afterwards -- far less error-prone, and it means a mislabelled session can
    be discarded wholesale rather than picked apart.

    Layout: metadata first, then 64 values named ``p00``..``p63`` in the same
    row-major order the firmware uses.
    """

    def __init__(self, path: str, label: str, sensor: str) -> None:
        self._handle: TextIO = open(path, "a", newline="")
        self._writer = csv.writer(self._handle)
        self._label = label
        self._count = 0

        # Only write a header to a new or empty file, so repeated sessions can
        # append to one file without interleaving headers into the data.
        if self._handle.tell() == 0:
            self._writer.writerow(
                ["timestamp", "sensor", "label", "frame"]
                + [f"p{i:02d}" for i in range(PIXELS)]
            )

    def write(self, frame: Frame) -> None:
        self._writer.writerow(
            [
                datetime.now(timezone.utc).isoformat(timespec="milliseconds"),
                frame.sensor,
                self._label,
                frame.number,
            ]
            + frame.values
        )
        self._count += 1
        # Flushed per frame on purpose: a session that ends with Ctrl-C or an
        # unplugged board should still have every frame it reported.
        self._handle.flush()

    @property
    def count(self) -> int:
        return self._count

    def close(self) -> None:
        self._handle.close()


class FusedCsvRecorder:
    """Append paired frames to a CSV, one row per pair -- the Phase 2 format.

    Layout: ``timestamp, label, frame, device_ms, die_cC, tilt_deg,
    roll_deg``, then
    ``d00..d63`` (raw ToF distance, mm), ``s00..s63`` (ULD target_status) and
    ``h00..h63`` (temperature, centidegrees C). All three grids row-major in
    the world frame, so index ``rc`` means the same patch of room in each.

    Distances are raw: where a zone's status is not valid the distance is not
    a measurement, and it is the reader's job to decide what to put there.
    Recording it this way costs nothing and keeps the decision reversible.
    The first frame after the ToF starts ranging is typically all-invalid;
    it is recorded like any other and is easy to drop by ``frame == 1``.
    """

    HEADER = (
        ["timestamp", "label", "frame", "device_ms", "die_cC", "tilt_deg", "roll_deg"]
        + [f"d{i:02d}" for i in range(PIXELS)]
        + [f"s{i:02d}" for i in range(PIXELS)]
        + [f"h{i:02d}" for i in range(PIXELS)]
    )

    def __init__(self, path: str, label: str) -> None:
        # Appending rows of one layout under another file's header would
        # shift every column silently. Refuse instead.
        try:
            with open(path, newline="") as existing:
                first = next(csv.reader(existing), None)
        except FileNotFoundError:
            first = None
        if first is not None and first != self.HEADER:
            raise SystemExit(
                f"{path} has a different column layout (older format?). "
                "Record to a new file."
            )
        self._handle: TextIO = open(path, "a", newline="")
        self._writer = csv.writer(self._handle)
        self._label = label
        self._count = 0
        if first is None:
            self._writer.writerow(self.HEADER)

    def write(self, fused: FusedFrame) -> None:
        self._writer.writerow(
            [
                datetime.now(timezone.utc).isoformat(timespec="milliseconds"),
                self._label,
                fused.number,
                "" if fused.device_ms is None else fused.device_ms,
                "" if fused.die is None else fused.die,
                "" if fused.tilt_deg is None else fused.tilt_deg,
                "" if fused.roll_deg is None else fused.roll_deg,
            ]
            + fused.distance
            + fused.status
            + fused.thermal
        )
        self._count += 1
        self._handle.flush()

    @property
    def count(self) -> int:
        return self._count

    def close(self) -> None:
        self._handle.close()


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Live 8x8 heatmap viewer for the CoworkOS sensors.",
        epilog=(
            "Typical use:\n"
            "  cargo run --release --bin tof_range | python3 tools/heatmap_viewer.py"
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--input",
        metavar="FILE",
        help="read from a saved log instead of stdin (replay, no hardware needed)",
    )
    parser.add_argument(
        "--record",
        metavar="CSV",
        help="append every frame to this CSV file",
    )
    parser.add_argument(
        "--label",
        default="unlabelled",
        help="class label written with each recorded frame, e.g. empty, sitting",
    )
    parser.add_argument(
        "--no-display",
        action="store_true",
        help="record only, draw nothing (for headless capture)",
    )
    parser.add_argument(
        "--no-values",
        action="store_true",
        help="omit the per-cell numbers, leaving just the colours",
    )
    args = parser.parse_args()

    source: TextIO = open(args.input) if args.input else sys.stdin

    if source.isatty():
        print(
            "Nothing is being piped in. Either pipe the firmware's output:\n"
            "  cargo run --release --bin tof_range | python3 tools/heatmap_viewer.py\n"
            "or replay a saved log with --input FILE.",
            file=sys.stderr,
        )
        return 2

    view = None
    recorder = None
    frames = 0
    tilt_start: Optional[float] = None
    tilt_alarmed = False

    try:
        for frame in assemble(parse_stream(source)):
            frames += 1

            if isinstance(frame, FusedFrame):
                # Mount drift: compare against this session's first reading.
                if frame.tilt_deg is not None:
                    if tilt_start is None:
                        tilt_start = frame.tilt_deg
                        print(f"mount tilt {tilt_start:.1f} deg at session start",
                              file=sys.stderr)
                    moved = abs(frame.tilt_deg - tilt_start)
                    if moved > TILT_ALARM_DEG and not tilt_alarmed:
                        print(
                            f"WARNING: mount tilt moved {moved:.1f} deg at frame "
                            f"{frame.number} ({tilt_start:.1f} -> "
                            f"{frame.tilt_deg:.1f}). The rig has been knocked; "
                            "frames from here may not match the rest.",
                            file=sys.stderr,
                        )
                        tilt_alarmed = True
                    elif moved <= TILT_ALARM_DEG / 2:
                        tilt_alarmed = False
                if recorder is None and args.record:
                    recorder = FusedCsvRecorder(args.record, args.label)
                    print(
                        f"recording paired frames to {args.record} as '{args.label}'",
                        file=sys.stderr,
                    )
                if view is None and not args.no_display:
                    try:
                        view = FusedView(annotate=not args.no_values)
                    except ImportError:
                        print(
                            "matplotlib is not installed, so no window will be shown.",
                            file=sys.stderr,
                        )
                        args.no_display = True
                if recorder is not None:
                    recorder.write(frame)
                if view is not None:
                    view.update(frame)
                continue

            # Both the view and the recorder are created on the first frame,
            # because the sensor type is not known until one arrives -- and it
            # decides the colour map, the units and the invalid-value sentinel.
            if recorder is None and args.record:
                recorder = CsvRecorder(args.record, args.label, frame.sensor)
                print(
                    f"recording {frame.sensor} frames to {args.record} "
                    f"as '{args.label}'",
                    file=sys.stderr,
                )

            if view is None and not args.no_display:
                style = STYLES.get(frame.sensor, FALLBACK_STYLE)
                try:
                    view = HeatmapView(style, annotate=not args.no_values)
                except ImportError:
                    print(
                        "matplotlib is not installed, so no window will be shown.\n"
                        "  pip install matplotlib numpy\n"
                        "Recording, if requested, continues regardless.",
                        file=sys.stderr,
                    )
                    args.no_display = True

            if recorder is not None:
                recorder.write(frame)
            if view is not None:
                view.update(frame)

    except KeyboardInterrupt:
        pass  # Ctrl-C is the normal way to end a session.
    finally:
        if recorder is not None:
            print(
                f"\nwrote {recorder.count} frames to {args.record}",
                file=sys.stderr,
            )
            recorder.close()
        if args.input:
            source.close()

    if frames == 0:
        print(
            "No frames found. Is the firmware running one of the grid-printing "
            "binaries (tof_range, thermal_probe, fused_stream)?",
            file=sys.stderr,
        )
        return 1

    if view is not None:
        view.keep_open()
    return 0


if __name__ == "__main__":
    sys.exit(main())
