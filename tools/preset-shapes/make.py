"""Make src/drawingml/presets.zip, the definitions drawingml::geometry draws
shapes and the text of WordArt with.

The preset shapes come from presetShapeDefinitions.xml (ECMA-376 Part 1, the
annex "Preset Shape Definitions"; LibreOffice has a copy at
oox/source/drawingml/customshapes/presetShapeDefinitions.xml), the text warps
of WordArt from presetTextWarpDefinitions.xml (the annex "Preset Text Warp
Definitions"; LibreOffice: oox/source/export/presetTextWarpDefinitions.xml).
Of each definition this keeps what drawing needs (avLst, gdLst, rect,
pathLst), without the namespaces and whitespace, as preset_shapes.xml and
preset_text_warps.xml in the zip (deflated: the crate embeds the zip, and
reads it on first use).

    python3 tools/preset-shapes/make.py presetShapeDefinitions.xml \\
        presetTextWarpDefinitions.xml

The zip is the same for the same definitions (fixed dates, fixed order), so
it changes only when they do. To see what it holds:

    unzip -p src/drawingml/presets.zip preset_shapes.xml
"""
import pathlib
import sys
import xml.etree.ElementTree as ET
import zipfile

KEEP = {"avLst", "gdLst", "rect", "pathLst"}
OUT = pathlib.Path(__file__).resolve().parents[2] / "src" / "drawingml" / "presets.zip"


def local(tag):
    return tag.rsplit("}", 1)[-1]


def out(e):
    attrs = "".join(f' {k}="{v}"' for k, v in e.attrib.items() if not k.startswith("{"))
    kids = "".join(out(c) for c in e)
    name = local(e.tag)
    return f"<{name}{attrs}>{kids}</{name}>" if kids else f"<{name}{attrs}/>"


def definitions(path, root_name):
    root = ET.parse(path).getroot()
    parts = []
    for shape in root:
        kids = "".join(out(c) for c in shape if local(c.tag) in KEEP)
        parts.append(f"<{local(shape.tag)}>{kids}</{local(shape.tag)}>")
    return f"<{root_name}>\n" + "\n".join(parts) + f"\n</{root_name}>\n"


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    entries = [
        ("preset_shapes.xml", definitions(sys.argv[1], "presetShapeDefinitions")),
        ("preset_text_warps.xml", definitions(sys.argv[2], "presetTextWarpDefinitions")),
    ]
    with zipfile.ZipFile(OUT, "w") as z:
        for name, text in entries:
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            z.writestr(info, text.encode("utf-8"), compresslevel=9)


main()
