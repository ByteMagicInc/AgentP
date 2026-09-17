"""Regenerate the README's four screenshots and three GIFs from the real TUI.

This is Linux-only documentation tooling, not part of the AgentP application.
Run from the repository root with network access for live RSS feeds:

    cargo build --locked
    python3 -m venv /tmp/agentp-media-venv
    /tmp/agentp-media-venv/bin/pip install Pillow pyte
    /tmp/agentp-media-venv/bin/python -B docs/capture_media.py

Install DejaVu Sans and Sans Mono, including Bold and Oblique faces, under
/usr/share/fonts/truetype/dejavu (Debian/Ubuntu: fonts-dejavu-core and
fonts-dejavu-extra). The script overwrites the seven files in docs/assets;
inspect them before committing. Episode titles and dates vary with live feeds.

Each walkthrough starts target/debug/agentp in a pseudo-terminal with a
disposable HOME seeded from example.podcasts.json. Keystrokes drive the app;
pyte interprets its terminal output and Pillow renders the cells into a framed
terminal image. Screen assertions reject unexpected states before capture.
Pauses sample live output to preserve animation, then frames become looping GIFs.

The walkthroughs browse and select episodes, filter the command palette and
visit configuration, and prepopulate The Pragmatic Engineer from its RSS feed.
They never download audio or edit the user's configuration.
"""

import codecs
import fcntl
import json
import os
import pty
import select
import struct
import subprocess
import tempfile
import termios
import time
from pathlib import Path

import pyte
from PIL import Image, ImageChops, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "docs/assets"
COLS, ROWS = 150, 38
CELL_W, CELL_H, PAD, OUTER, TITLE_H = 10, 24, 18, 18, 48
BG, PAGE_BG, TITLE_BG = "#282a36", "#191a21", "#21222c"
FONTS = Path("/usr/share/fonts/truetype/dejavu")
FONT = ImageFont.truetype(str(FONTS / "DejaVuSansMono.ttf"), 17)
BOLD = ImageFont.truetype(str(FONTS / "DejaVuSansMono-Bold.ttf"), 17)
ITALIC = ImageFont.truetype(str(FONTS / "DejaVuSansMono-Oblique.ttf"), 17)
TITLE_FONT = ImageFont.truetype(str(FONTS / "DejaVuSans.ttf"), 16)
FEATURED = [
    "The AI Daily Brief",
    "Merge Conflict",
    "Committing High Reason",
    "The Pragmatic Engineer",
]
ANSI = dict(
    zip(
        [
            "black",
            "red",
            "green",
            "brown",
            "blue",
            "magenta",
            "cyan",
            "white",
            "brightblack",
            "brightred",
            "brightgreen",
            "brightbrown",
            "brightblue",
            "brightmagenta",
            "brightcyan",
            "brightwhite",
        ],
        [
            "#21222c",
            "#ff5555",
            "#50fa7b",
            "#f1fa8c",
            "#6272a4",
            "#ff79c6",
            "#8be9fd",
            "#f8f8f2",
            "#6272a4",
            "#ff6e6e",
            "#69ff94",
            "#ffffa5",
            "#d6acff",
            "#ff92df",
            "#a4ffff",
            "#ffffff",
        ],
    )
)


def color(value, default):
    """Resolve pyte's default, named ANSI, or hexadecimal color for Pillow."""
    if value == "default":
        return default
    return ANSI.get(value, value if value.startswith("#") else "#" + value)


def draw_glyph(draw, x, y, ch, fg):
    """Draw terminal strokes edge-to-edge; use fonts for ordinary text.

    Box and block characters need full-cell geometry so the banner and borders
    connect without the gaps introduced by a font's glyph spacing.
    """
    right, bottom = x + CELL_W - 1, y + CELL_H - 1
    cx, cy = x + CELL_W // 2, y + CELL_H // 2
    if ch.data in "─│┌┐└┘├┤┬┴┼╭╮╰╯":
        directions = {
            "─": "lr",
            "│": "ud",
            "┌": "rd",
            "┐": "ld",
            "└": "ru",
            "┘": "lu",
            "├": "rud",
            "┤": "lud",
            "┬": "lrd",
            "┴": "lru",
            "┼": "lrud",
            "╭": "rd",
            "╮": "ld",
            "╰": "ru",
            "╯": "lu",
        }[ch.data]
        if ch.data in "╭╮╰╯":
            if "d" in directions:
                draw.line((cx, cy + 5, cx, bottom), fill=fg)
            else:
                draw.line((cx, y, cx, cy - 5), fill=fg)
            arcs = {
                "╭": ((cx, cy, cx + 10, cy + 10), 180, 270),
                "╮": ((cx - 10, cy, cx, cy + 10), 270, 360),
                "╰": ((cx, cy - 10, cx + 10, cy), 90, 180),
                "╯": ((cx - 10, cy - 10, cx, cy), 0, 90),
            }
            box, start, end = arcs[ch.data]
            draw.arc(box, start, end, fill=fg)
        else:
            for direction, endpoint in (
                ("l", (x, cy)),
                ("r", (right, cy)),
                ("u", (cx, y)),
                ("d", (cx, bottom)),
            ):
                if direction in directions:
                    draw.line((cx, cy, *endpoint), fill=fg)
    elif ch.data in "▁▂▃▄▅▆▇█":
        height = ("▁▂▃▄▅▆▇█".index(ch.data) + 1) * CELL_H // 8
        draw.rectangle((x, y + CELL_H - height, right, bottom), fill=fg)
    elif ch.data == "▀":
        draw.rectangle((x, y, right, y + CELL_H // 2 - 1), fill=fg)
    elif ch.data in "╱╲":
        ends = (x, bottom, right, y) if ch.data == "╱" else (x, y, right, bottom)
        draw.line(ends, fill=fg)
    else:
        font = BOLD if ch.bold else ITALIC if ch.italics else FONT
        draw.text((x, y), ch.data, font=font, fill=fg)


class Terminal:
    """Own one isolated TUI process, its emulated screen, and captured frames."""

    def __init__(self, omit=None):
        self.home = tempfile.TemporaryDirectory(prefix="agentp-media-")
        config = Path(self.home.name) / ".config/AgentP"
        config.mkdir(parents=True)
        (config / "config.json").write_text(
            json.dumps(
                {
                    "download_dir_location": "~/Downloads/Podcasts",
                    "default_mode": "tui",
                }
            )
        )
        source = json.loads((ROOT / "example.podcasts.json").read_text())["podcasts"]
        by_name = {p["name"]: p for p in source}
        ordered = [by_name[name] for name in FEATURED] + [
            p for p in source if p["name"] not in FEATURED
        ]
        (config / "podcasts.json").write_text(
            json.dumps({"podcasts": [p for p in ordered if p["name"] != omit]})
        )
        self.screen = pyte.Screen(COLS, ROWS)
        self.stream = pyte.Stream(self.screen)
        self.decoder = codecs.getincrementaldecoder("utf-8")()
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        env = os.environ.copy()
        for key in ("NO_COLOR", "CLICOLOR", "CLICOLOR_FORCE", "FORCE_COLOR"):
            env.pop(key, None)
        env.update(HOME=self.home.name, TERM="xterm-256color", COLORTERM="truecolor")
        self.process = subprocess.Popen(
            [str(ROOT / "target/debug/agentp"), "--tui"],
            stdin=slave,
            stdout=slave,
            stderr=slave,
            env=env,
        )
        os.close(slave)
        self.frames, self.durations = [], []
        self.last_frame = None

    def __enter__(self):
        self.wait_for(lambda: "Podcasts" in self.text())
        self.drain(10)
        self.send(b"?")
        self.wait_for(
            lambda: all(
                any(
                    name in row and len(row.split(name, 1)[1].strip(" │▲█║▼")) > 15
                    for row in self.screen.display
                )
                for name in FEATURED
                if name != "The Pragmatic Engineer"
            )
        )
        return self

    def __exit__(self, *args):
        try:
            self.send(b"\x03")
            self.process.wait(timeout=3)
        finally:
            if self.process.poll() is None:
                self.process.terminate()
                self.process.wait(timeout=3)
            os.close(self.master)
            self.home.cleanup()

    def drain(self, seconds=0.15):
        """Consume terminal output for the given number of seconds."""
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            ready, _, _ = select.select(
                [self.master], [], [], min(0.05, max(0, end - time.monotonic()))
            )
            if ready:
                try:
                    data = os.read(self.master, 65536)
                except OSError:
                    return
                self.stream.feed(self.decoder.decode(data))

    def send(self, data, wait=0.15):
        """Send raw keystrokes, then allow the TUI to redraw."""
        os.write(self.master, data)
        self.drain(wait)

    def text(self):
        return "\n".join(self.screen.display)

    def wait_for(self, predicate, timeout=30):
        """Wait for a screen condition; include the screen text on timeout."""
        end = time.monotonic() + timeout
        while not predicate():
            if time.monotonic() > end:
                raise AssertionError(self.text())
            self.drain()

    def render(self):
        """Render the current terminal cells and decorative window frame."""
        body_w, body_h = COLS * CELL_W + PAD * 2, ROWS * CELL_H + PAD * 2
        content = Image.new("RGB", (body_w, body_h), BG)
        draw = ImageDraw.Draw(content)
        glyphs = []
        for y in range(ROWS):
            for x in range(COLS):
                ch = self.screen.buffer[y][x]
                fg, bg = color(ch.fg, "#f8f8f2"), color(ch.bg, BG)
                if ch.reverse:
                    fg, bg = bg, fg
                px, py = PAD + x * CELL_W, PAD + y * CELL_H
                draw.rectangle((px, py, px + CELL_W - 1, py + CELL_H - 1), fill=bg)
                if ch.data.strip():
                    glyphs.append((px, py, ch, fg))
        for px, py, ch, fg in glyphs:
            draw_glyph(draw, px, py, ch, fg)
        width, height = body_w + OUTER * 2, body_h + TITLE_H + OUTER * 2
        image = Image.new("RGB", (width, height), PAGE_BG)
        d = ImageDraw.Draw(image)
        d.rounded_rectangle(
            (OUTER, OUTER, width - OUTER - 1, height - OUTER - 1), radius=12, fill=BG
        )
        d.rounded_rectangle(
            (OUTER, OUTER, width - OUTER - 1, OUTER + TITLE_H + 10),
            radius=12,
            fill=TITLE_BG,
        )
        for cx, c in zip(
            (OUTER + 20, OUTER + 40, OUTER + 60), ("#ff5f56", "#ffbd2e", "#27c93f")
        ):
            d.ellipse(
                (cx - 6, OUTER + TITLE_H // 2 - 6, cx + 6, OUTER + TITLE_H // 2 + 6),
                fill=c,
            )
        title = "AgentP — Terminal"
        d.text(
            ((width - d.textlength(title, font=TITLE_FONT)) / 2, OUTER + 10),
            title,
            font=TITLE_FONT,
            fill="#bfbfc7",
        )
        image.paste(content, (OUTER, OUTER + TITLE_H))
        return image

    def capture(self, duration=1000, screenshot=None, expected=()):
        """Check a state and record a pause, with duration in milliseconds.

        Save the initial frame as a PNG when requested; sample subsequent
        output at 100 ms intervals for the GIF's live banner animation. Merge
        identical samples and retain changed frames as indexed-color images,
        keeping only the latest RGB frame for lossless comparison.
        """
        text = self.text()
        for value in expected:
            assert value in text, (value, text)
        assert " more" not in "\n".join(self.screen.display[-4:]), text
        assert "Loading" not in text, text
        assert "Error:" not in text, text
        frame = self.render()
        if screenshot:
            frame.save(ASSETS / screenshot)
        for elapsed in range(0, duration, 100):
            if elapsed:
                self.drain(0.1)
                frame = self.render()
            hold = min(100, duration - elapsed)
            if (
                self.last_frame is not None
                and ImageChops.difference(self.last_frame, frame).getbbox() is None
            ):
                self.durations[-1] += hold
            else:
                self.frames.append(frame.convert("P", palette=Image.Palette.ADAPTIVE))
                self.durations.append(hold)
                self.last_frame = frame

    def gif(self, name):
        """Write the accumulated frames as an optimized, looping GIF."""
        self.frames[0].save(
            ASSETS / name,
            save_all=True,
            append_images=self.frames[1:],
            duration=self.durations,
            loop=0,
            optimize=True,
            disposal=1,
        )
        print(
            f"{name}: {len(self.frames)} frames, all state checks passed, {sum(self.durations) / 1000:.1f}s",
            flush=True,
        )

    def episodes(self, name):
        """Open a podcast and record selecting its first and third episodes."""
        self.send(b"\r")
        self.wait_for(
            lambda: "Loading episodes" not in self.text() and "[ ]" in self.text()
        )
        self.capture(1400, expected=(name, "[ ]"))
        self.send(b" ")
        self.capture(650, expected=("[x]",))
        self.send(b"jj ")
        self.capture(1800, expected=("2/",))


def browse():
    """Capture three featured podcasts plus library and selection screenshots."""
    with Terminal() as t:
        t.capture(1800, "podcast-list.png", FEATURED)
        t.episodes("The AI Daily Brief")
        t.send(b"\x1b")
        t.send(b"j")
        t.capture(1000, expected=("Merge Conflict",))
        t.episodes("Merge Conflict")
        t.send(b"\x1b")
        t.send(b"j")
        t.capture(1000)
        t.episodes("Committing High Reason")
        t.capture(500, "episode-selection.png", ("Committing High Reason", "2/"))
        t.send(b"\x1b")
        t.send(b"g")
        t.capture(1200)
        t.gif("browse-and-select.gif")


def palette():
    """Capture folder-action filtering and configuration navigation."""
    with Terminal() as t:
        t.send(b"jjj")
        t.capture(1000)
        t.send(b"\x0b")
        t.capture(1100, expected=("Command",))
        for ch in b"folder":
            t.send(bytes([ch]))
            t.capture(150)
        t.capture(1700, "command-palette.png", ("folder",))
        t.send(b"\x1b")
        t.send(b"c")
        t.capture(1200, expected=("Default Mode",))
        t.send(b"j")
        t.capture(1600, "configuration.png", ("Download Folder",))
        t.send(b"j")
        t.capture(1200, expected=("Default Mode",))
        t.send(b"\x1b")
        t.capture(900)
        t.gif("command-palette-and-config.gif")


def add():
    """Capture RSS prepopulation through the name, album, and artist steps."""
    source = json.loads((ROOT / "example.podcasts.json").read_text())["podcasts"]
    feed = next(p["feed_url"] for p in source if p["name"] == "The Pragmatic Engineer")
    with Terminal(omit="The Pragmatic Engineer") as t:
        t.send(b"\x0b")
        t.send(b"add")
        t.capture(1000, expected=("add",))
        t.send(b"\r")
        t.capture(1000, expected=("Feed URL",))
        for start in range(0, len(feed), 8):
            t.send(feed[start : start + 8].encode())
            t.capture(150)
        t.capture(1000, expected=(feed,))
        t.send(b"\r")
        t.capture(1500, expected=("Manual", "Prepopulate"))
        t.send(b"p")
        t.wait_for(
            lambda: "The Pragmatic Engineer" in t.text() and "Fetching" not in t.text()
        )
        t.capture(1800, expected=("Name", "The Pragmatic Engineer"))
        t.send(b"\r")
        t.capture(1400, expected=("Album Name", "The Pragmatic Engineer"))
        t.send(b"\r")
        t.capture(1800, expected=("Artist", "Gergely"))
        t.gif("add-from-feed.gif")


if __name__ == "__main__":
    browse()
    palette()
    add()
