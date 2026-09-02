#!/usr/bin/env python3
"""Generates the marketing site's favicon set from the app's own icon.

The site had no favicon at all — not a broken one, none: no `<link
rel="icon">` and no file, so every browser tab showed a blank page glyph
next to a product that has a perfectly good mark. `/favicon.ico`,
`/favicon.png` and `/apple-touch-icon.png` all answered 404.

Generated rather than copied, from `apps/desktop/src-tauri/icons/icon.png`
— the same 512px source the desktop app and the web app already use, so
the tab icon, the dock icon and the home-screen icon cannot drift into
three slightly different reds.

Run from the repo root:  python3 site/scripts/build-icons.py [--check]
"""
import sys
from pathlib import Path

from PIL import Image

SOURCE = Path("apps/desktop/src-tauri/icons/icon.png")
OUT = Path("site")

# The icon's own background, sampled from the source. Used only to
# flatten the apple-touch-icon: iOS composites that one over white and
# applies its own rounded mask, so shipping the source's transparent
# corners would put white triangles behind a red tile.
BACKGROUND = (255, 77, 77)

# 48 is not vestigial: Windows still uses it for pinned-site and taskbar
# shortcuts, where a 32 upscaled looks visibly soft.
ICO_SIZES = [(16, 16), (32, 32), (48, 48)]
PNG_SIZE = 32
APPLE_SIZE = 180


def render() -> dict[Path, bytes]:
    """Everything this script would write, as bytes, without writing."""
    import io

    source = Image.open(SOURCE).convert("RGBA")
    out: dict[Path, bytes] = {}

    buf = io.BytesIO()
    source.save(buf, format="ICO", sizes=ICO_SIZES)
    out[OUT / "favicon.ico"] = buf.getvalue()

    buf = io.BytesIO()
    source.resize((PNG_SIZE, PNG_SIZE), Image.LANCZOS).save(buf, format="PNG", optimize=True)
    out[OUT / "favicon.png"] = buf.getvalue()

    apple = Image.new("RGB", (APPLE_SIZE, APPLE_SIZE), BACKGROUND)
    scaled = source.resize((APPLE_SIZE, APPLE_SIZE), Image.LANCZOS)
    apple.paste(scaled, (0, 0), scaled)
    buf = io.BytesIO()
    apple.save(buf, format="PNG", optimize=True)
    out[OUT / "apple-touch-icon.png"] = buf.getvalue()

    return out


def main() -> None:
    if not SOURCE.exists():
        sys.exit(f"build-icons.py: no source icon at {SOURCE}")

    rendered = render()

    # PNG and ICO encoders are not byte-reproducible across Pillow
    # versions, so --check compares presence and pixels, not bytes: the
    # question worth failing on is "did someone change the app icon and
    # forget the site", not "was this built by a different Pillow".
    if "--check" in sys.argv:
        for path in rendered:
            if not path.exists():
                sys.exit(f"{path} is missing — run: python3 site/scripts/build-icons.py")
        source = Image.open(SOURCE).convert("RGBA").resize((32, 32), Image.LANCZOS)
        current = Image.open(OUT / "favicon.png").convert("RGBA").resize((32, 32), Image.LANCZOS)
        if source.tobytes() != current.tobytes():
            sys.exit("site/favicon.png no longer matches the app icon — rerun this script")
        print("site icons are up to date")
        return

    for path, data in rendered.items():
        path.write_bytes(data)
        print(f"wrote {path} ({len(data) / 1024:.1f} KB)")


if __name__ == "__main__":
    main()
