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

Record frames to CSV at the same time, which is what Phase 2 needs::

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

_FRAME_HEADER = re.compile(r"---\s*frame\s+(\d+)\s*---")

# Eight whitespace-separated integers immediately before the ASCII map, which
# is the part that makes a row line unambiguous. Matching on "eight integers"
# alone would also catch the header's "137..261".
_FRAME_ROW = re.compile(r"\s((?:-?\d+\s+){%d})\|" % GRID)

# defmt tags each line with the binary that emitted it, which is a more
# reliable sensor identifier than guessing from the header's wording.
_SOURCE = re.compile(r"\((tof_range|thermal_probe)\s")


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
    """Either ``tof_range`` or ``thermal_probe``."""

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

        frame = Frame(
            number=self._number,
            values=[v for row_values in self._rows for v in row_values],
            sensor=self._sensor,
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

    def __init__(self, style: SensorStyle, annotate: bool = True) -> None:
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
        self._figure, self._axes = plt.subplots(figsize=(6.5, 7.0))

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
        self._figure.tight_layout()

    def update(self, frame: Frame) -> None:
        """Redraw with a new frame."""
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

    view: Optional[HeatmapView] = None
    recorder: Optional[CsvRecorder] = None
    frames = 0

    try:
        for frame in parse_stream(source):
            frames += 1

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
            "binaries (tof_range, thermal_probe)?",
            file=sys.stderr,
        )
        return 1

    if view is not None:
        view.keep_open()
    return 0


if __name__ == "__main__":
    sys.exit(main())
