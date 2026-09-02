#!/usr/bin/env python3
"""从 resources/icons/source/ZzClawTerm.png 一键生成三平台图标产物。

用法：python3 scripts/generate-icons.py
生成后自动回读校验（ico 尺寸齐全、icns 条目表完整、png 可解析）。
"""
import io
import struct
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "resources" / "icons" / "source" / "ZzClawTerm.png"
OUT = ROOT / "resources" / "icons" / "generated"

ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]
PNG_SIZES = [16, 24, 32, 48, 64, 128, 256, 512]
# (类型码, 边长)；icp4/5/6 为 1x 小尺寸组，ic07-ic10 为 128-1024，ic11-ic14 为 @2x 槽位
ICNS_ENTRIES = [
    ("icp4", 16), ("icp5", 32), ("icp6", 64), ("ic07", 128),
    ("ic08", 256), ("ic09", 512), ("ic10", 1024),
    ("ic11", 64), ("ic12", 32), ("ic13", 256), ("ic14", 512),
]
NEED_SIZES = sorted({s for _, s in ICNS_ENTRIES} | set(PNG_SIZES) | set(ICO_SIZES))


def render() -> dict[int, Image.Image]:
    img = Image.open(SOURCE).convert("RGBA")
    return {s: img.resize((s, s), Image.LANCZOS) for s in NEED_SIZES}


def write_pngs(images: dict[int, Image.Image]) -> None:
    png_dir = OUT / "png"
    png_dir.mkdir(parents=True, exist_ok=True)
    for s in PNG_SIZES:
        images[s].save(png_dir / f"{s}.png", optimize=True)


def write_ico(images: dict[int, Image.Image]) -> None:
    # Pillow 的 ICO 保存按 sizes 列表用 LANCZOS 派生各尺寸，
    # 且会跳过大于源图尺寸的条目——必须从 >=256 的图保存
    images[max(ICO_SIZES)].save(
        OUT / "zzclawterm.ico", format="ICO",
        sizes=[(s, s) for s in ICO_SIZES],
    )


def write_icns(images: dict[int, Image.Image]) -> None:
    body = b""
    for code, s in ICNS_ENTRIES:
        buf = io.BytesIO()
        images[s].save(buf, format="PNG", optimize=True)
        data = buf.getvalue()
        body += code.encode("ascii") + struct.pack(">I", 8 + len(data)) + data
    (OUT / "zzclawterm.icns").write_bytes(
        b"icns" + struct.pack(">I", 8 + len(body)) + body)


def verify() -> None:
    with Image.open(OUT / "zzclawterm.ico") as ico:
        # info["sizes"] 须在 load() 前读取（load 后会被当前帧覆盖）
        declared = set(ico.info.get("sizes", set()))
        want = {(s, s) for s in ICO_SIZES}
        assert declared == want, f"ico 尺寸声明不全: 缺失 {want - declared}"
        ico.load()

    raw = (OUT / "zzclawterm.icns").read_bytes()
    assert raw[:4] == b"icns", "icns magic 错误"
    total = struct.unpack(">I", raw[4:8])[0]
    assert total == len(raw), f"icns 长度字段 {total} != 实际 {len(raw)}"
    codes = set()
    off = 8
    while off < total:
        code = raw[off:off + 4].decode("ascii")
        size = struct.unpack(">I", raw[off + 4:off + 8])[0]
        assert size >= 8 and off + size <= total, f"icns 条目 {code} 长度非法"
        Image.open(io.BytesIO(raw[off + 8:off + size])).verify()
        codes.add(code)
        off += size
    want = {c for c, _ in ICNS_ENTRIES}
    assert codes == want, f"icns 条目缺失: {want - codes}"

    for s in PNG_SIZES:
        with Image.open(OUT / "png" / f"{s}.png") as im:
            im.verify()
    print("校验通过：ico 7 尺寸 / icns 11 条目 / png 8 尺寸")


def main() -> int:
    if not SOURCE.exists():
        print(f"源图不存在: {SOURCE}", file=sys.stderr)
        return 1
    images = render()
    write_pngs(images)
    write_ico(images)
    write_icns(images)
    verify()
    print(f"产物目录: {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
