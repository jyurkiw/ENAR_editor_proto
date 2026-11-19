"""Placeholder art for the Grand Forks test supplement.

Every picture is a flat colour with a simple shape and its title, stamped SLOP ART corner to
corner so nobody mistakes it for real art. Made fresh on every run, never committed.

    python make_slop_art.py OUT_DIR
"""
import math
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

WATERMARK = 'SLOP ART'

# name: (size, background, title, shape)
ART = {
    'cover.png': ((1275, 1650), (52, 84, 110), 'Grand Forks', 'fork'),
    'grand-forks-map.png': ((1600, 1200), (196, 178, 140), 'Map of Grand Forks', 'map'),
    'toll-bridge.png': ((1200, 800), (88, 104, 74), 'The Toll Bridge', 'bridge'),
    'forkjaw-pike.png': ((900, 1200), (40, 70, 80), 'Forkjaw Pike', 'fish'),
    'lantern.png': ((600, 600), (60, 48, 70), "Ferryman's Lantern", 'lantern'),
}


def font(size):
    for name in ('arialbd.ttf', 'DejaVuSans-Bold.ttf', 'Arial Bold.ttf'):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            pass
    return ImageFont.load_default(size)


def shape(d, kind, w, h, ink):
    lw = max(4, w // 120)
    if kind in ('fork', 'map'):
        # A river coming in from the top and splitting around a wedge.
        d.line([(w * .5, 0), (w * .5, h * .45)], fill=ink, width=lw * 4)
        d.line([(w * .5, h * .45), (w * .15, h)], fill=ink, width=lw * 3)
        d.line([(w * .5, h * .45), (w * .85, h)], fill=ink, width=lw * 3)
        if kind == 'map':
            d.line([(0, h * .3), (w, h * .3)], fill=(120, 80, 40), width=lw)      # the Copperway
            for x, y in ((.3, .2), (.7, .7), (.5, .8)):
                d.rectangle([w * x - 30, h * y - 30, w * x + 30, h * y + 30], outline=ink, width=lw)
    elif kind == 'bridge':
        d.arc([w * .1, h * .35, w * .9, h * 1.1], 180, 360, fill=ink, width=lw * 2)
        d.line([(0, h * .72), (w, h * .72)], fill=ink, width=lw)
    elif kind == 'fish':
        d.ellipse([w * .15, h * .35, w * .8, h * .6], outline=ink, width=lw)
        d.polygon([(w * .8, h * .475), (w * .95, h * .38), (w * .95, h * .57)], outline=ink, width=lw)
    elif kind == 'lantern':
        d.rectangle([w * .35, h * .3, w * .65, h * .75], outline=ink, width=lw)
        d.arc([w * .4, h * .15, w * .6, h * .4], 180, 360, fill=ink, width=lw)


def watermark(img, rows):
    """SLOP ART across the picture, corner to corner (twice on a tall one)."""
    w, h = img.size
    diag = math.hypot(w, h / rows)
    layer = Image.new('RGBA', (int(diag), int(diag / 4)), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    size = 100
    box = d.textbbox((0, 0), WATERMARK, font=font(size))
    size = int(size * diag * .8 / (box[2] - box[0]))        # 80% of the diagonal
    f = font(size)
    box = d.textbbox((0, 0), WATERMARK, font=f)
    d.text(((layer.width - (box[2] - box[0])) / 2 - box[0], (layer.height - (box[3] - box[1])) / 2 - box[1]),
           WATERMARK, font=f, fill=(255, 255, 255, 110), stroke_width=max(2, w // 300), stroke_fill=(0, 0, 0, 110))
    angle = math.degrees(math.atan2(h / rows, w))
    stamp = layer.rotate(angle, expand=True, resample=Image.BICUBIC)
    for r in range(rows):
        cy = h / rows * (r + .5)
        img.alpha_composite(stamp, (int(w / 2 - stamp.width / 2), int(cy - stamp.height / 2)))


def make(name, size, bg, title, kind):
    w, h = size
    img = Image.new('RGBA', size, bg + (255,))
    d = ImageDraw.Draw(img)
    ink = tuple(min(255, c + 90) for c in bg)
    shape(d, kind, w, h, ink)
    d.text((w * .05, h * .04), title, font=font(max(18, w // 22)), fill=(255, 255, 255))
    watermark(img, 2 if h > w * 1.2 else 1)
    return img.convert('RGB')


def main(out):
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    for name, (size, bg, title, kind) in ART.items():
        make(name, size, bg, title, kind).save(out / name)
    return [out / n for n in ART]


if __name__ == '__main__':
    for p in main(sys.argv[1] if len(sys.argv) > 1 else '.'):
        print(p)
