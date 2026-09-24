//! 程序化生成程序图标：深色圆角底 + 三条彩色横条（象征 JSONL 的三行记录，
//! 配色取自应用内 wire 事件着色：绿/黄/蓝）。

/// 渲染指定边长的正方形图标，返回 RGBA 像素。
pub fn render_rgba(size: usize) -> Vec<u8> {
    let s = size as f32;
    let mut rgba = vec![0u8; size * size * 4];
    let mut set_px = |x: usize, y: usize, c: [u8; 4]| {
        let i = (y * size + x) * 4;
        rgba[i..i + 4].copy_from_slice(&c);
    };

    // 背景：深色圆角方块（留约 3% 透明边）
    let m = s * 2.0 / 64.0;
    let r = s * 14.0 / 64.0;
    for y in 0..size {
        for x in 0..size {
            if in_rounded(x as f32 + 0.5, y as f32 + 0.5, m, m, s - m, s - m, r) {
                set_px(x, y, [0x1e, 0x1e, 0x2e, 0xff]);
            }
        }
    }
    // 三条「JSONL 行」，长短不一
    let bars: [(f32, f32, f32, [u8; 4]); 3] = [
        (12.0, 15.0, 40.0, [0xa6, 0xda, 0x95, 0xff]), // 绿
        (12.0, 29.0, 30.0, [0xee, 0x99, 0x28, 0xff]), // 黄
        (12.0, 43.0, 36.0, [0x61, 0xaf, 0xef, 0xff]), // 蓝
    ];
    let bar_h = s * 6.0 / 64.0;
    for (bx, by, blen, color) in bars {
        let (x0, y0, x1, y1) = (
            s * bx / 64.0,
            s * by / 64.0,
            s * (bx + blen) / 64.0,
            s * by / 64.0 + bar_h,
        );
        let br = bar_h / 2.0;
        for y in (y0 as usize)..=(y1 as usize).min(size - 1) {
            for x in (x0 as usize)..=(x1 as usize).min(size - 1) {
                if in_rounded(x as f32 + 0.5, y as f32 + 0.5, x0, y0, x1, y1, br) {
                    set_px(x, y, color);
                }
            }
        }
    }
    rgba
}

fn in_rounded(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

/// 把 RGBA 像素打包成 .ico 文件内容（BMP 条目，多尺寸）。
/// 供 examples/make_icon.rs 使用。
#[allow(dead_code)]
pub fn make_ico(sizes: &[usize]) -> Vec<u8> {
    let mut out = Vec::new();
    // ICONDIR
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type = icon
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());

    let mut entries = Vec::new();
    let mut images = Vec::new();
    let mut offset = 6 + 16 * sizes.len();
    for &sz in sizes {
        let rgba = render_rgba(sz);
        let img = ico_bmp_entry(&rgba, sz);
        entries.push((sz, img.len() as u32, offset as u32));
        images.push(img);
        offset += images.last().unwrap().len();
    }
    for (sz, bytes_in_res, off) in entries {
        out.push(if sz >= 256 { 0 } else { sz as u8 }); // width (0 = 256)
        out.push(if sz >= 256 { 0 } else { sz as u8 }); // height
        out.push(0); // color count
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bit count
        out.extend_from_slice(&bytes_in_res.to_le_bytes());
        out.extend_from_slice(&off.to_le_bytes());
    }
    for img in images {
        out.extend_from_slice(&img);
    }
    out
}

/// ICO 内嵌 BMP：BITMAPINFOHEADER（高度 ×2 含掩码）+ 自底向上 BGRA + 全零 AND 掩码。
#[allow(dead_code)]
fn ico_bmp_entry(rgba: &[u8], size: usize) -> Vec<u8> {
    let xor_bytes = size * size * 4;
    let mask_row_bytes = (size + 31) / 32 * 4; // 1bpp，行对齐 4 字节
    let and_bytes = mask_row_bytes * size;
    let mut out = Vec::with_capacity(40 + xor_bytes + and_bytes);
    // BITMAPINFOHEADER
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // 含 AND 掩码
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bpp
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&((xor_bytes + and_bytes) as u32).to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes()); // x ppm
    out.extend_from_slice(&0i32.to_le_bytes()); // y ppm
    out.extend_from_slice(&0u32.to_le_bytes()); // colors used
    out.extend_from_slice(&0u32.to_le_bytes()); // important colors
    // 像素：自底向上，BGRA
    for y in (0..size).rev() {
        for x in 0..size {
            let i = (y * size + x) * 4;
            out.push(rgba[i + 2]); // B
            out.push(rgba[i + 1]); // G
            out.push(rgba[i]); // R
            out.push(rgba[i + 3]); // A
        }
    }
    out.extend(std::iter::repeat(0u8).take(and_bytes)); // AND 掩码全 0 = 全部不透明
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_rgba_size() {
        let px = render_rgba(64);
        assert_eq!(px.len(), 64 * 64 * 4);
        // 右上角背景像素（避开三条横条区域）
        let c = (8 * 64 + 56) * 4;
        assert_eq!(&px[c..c + 4], &[0x1e, 0x1e, 0x2e, 0xff]);
        // 角上应透明
        assert_eq!(&px[0..4], &[0, 0, 0, 0]);
        // 绿色横条区域
        let g = (17 * 64 + 20) * 4;
        assert_eq!(&px[g..g + 4], &[0xa6, 0xda, 0x95, 0xff]);
        // 黄色横条区域
        let y = (31 * 64 + 20) * 4;
        assert_eq!(&px[y..y + 4], &[0xee, 0x99, 0x28, 0xff]);
    }

    #[test]
    fn ico_structure() {
        let ico = make_ico(&[16, 32]);
        assert_eq!(&ico[0..4], &[0, 0, 1, 0]);
        assert_eq!(u16::from_le_bytes([ico[4], ico[5]]), 2);
        assert_eq!(ico[6], 16); // 第一个条目宽 16
        assert_eq!(ico[6 + 16], 32); // 第二个条目宽 32
    }
}
