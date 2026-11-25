//! Minimal pure-Rust PNG and zlib/deflate (RFC 1950 + RFC 1951) decoder for
//! splitting vanilla/resource-pack `rain.png` and `snow.png` into weather particle sprites.

/// Decoded RGBA8 image (`width * height * 4` bytes, row-major, top-to-bottom).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
const MAX_DIM: u32 = 4096;
const MAX_RAW_BYTES: usize = 64 * 1024 * 1024;

/// Decode an 8-bit non-interlaced PNG into RGBA8 pixels.
pub fn decode_png(data: &[u8]) -> Result<RgbaImage, String> {
    if data.len() < 8 || data[..8] != PNG_SIG {
        return Err("invalid PNG signature".into());
    }

    let mut pos = 8usize;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut color_type = 0u8;
    let mut saw_ihdr = false;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut trns: Vec<u8> = Vec::new();
    let mut idat = Vec::new();

    while pos + 12 <= data.len() {
        let len =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let kind = &data[pos + 4..pos + 8];
        let payload_start = pos + 8;
        let Some(payload_end) = payload_start.checked_add(len) else {
            return Err("PNG chunk length overflow".into());
        };
        let Some(next_pos) = payload_end.checked_add(4) else {
            return Err("PNG chunk CRC overflow".into());
        };
        if next_pos > data.len() {
            return Err("truncated PNG chunk".into());
        }
        let payload = &data[payload_start..payload_end];
        pos = next_pos;

        match kind {
            b"IHDR" => {
                if payload.len() < 13 {
                    return Err("truncated IHDR".into());
                }
                width = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                height = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
                let bit_depth = payload[8];
                color_type = payload[9];
                let compression = payload[10];
                let filter = payload[11];
                let interlace = payload[12];
                if width == 0 || height == 0 || width > MAX_DIM || height > MAX_DIM {
                    return Err(format!("unsupported PNG dimensions {width}x{height}"));
                }
                if compression != 0 || filter != 0 || interlace != 0 {
                    return Err("unsupported PNG compression/filter/interlace mode".into());
                }
                if bit_depth != 8 {
                    return Err(format!("unsupported PNG bit depth {bit_depth}"));
                }
                saw_ihdr = true;
            }
            b"PLTE" => {
                if !payload.len().is_multiple_of(3) {
                    return Err("invalid PLTE length".into());
                }
                palette = payload.as_chunks::<3>().0.to_vec();
            }
            b"tRNS" => {
                trns = payload.to_vec();
            }
            b"IDAT" => {
                if idat.len().saturating_add(payload.len()) > MAX_RAW_BYTES {
                    return Err("IDAT stream too large".into());
                }
                idat.extend_from_slice(payload);
            }
            b"IEND" => break,
            _ => {}
        }
    }

    if !saw_ihdr || idat.is_empty() {
        return Err("missing IHDR or IDAT".into());
    }

    let bpp: usize = match color_type {
        0 => 1, // Grayscale
        2 => 3, // RGB
        3 => 1, // Indexed
        4 => 2, // Grayscale + Alpha
        6 => 4, // RGBA
        _ => return Err(format!("unsupported PNG color type {color_type}")),
    };

    let row_bytes = (width as usize)
        .checked_mul(bpp)
        .ok_or_else(|| "row size overflow".to_string())?;
    let expected_raw = (height as usize)
        .checked_mul(row_bytes + 1)
        .ok_or_else(|| "image size overflow".to_string())?;
    if expected_raw > MAX_RAW_BYTES {
        return Err("decoded PNG exceeds size limit".into());
    }

    let raw = zlib_decompress(&idat, expected_raw)?;
    if raw.len() < expected_raw {
        return Err("decompressed PNG data shorter than expected".into());
    }

    let mut prev_row = vec![0u8; row_bytes];
    let mut curr_row = vec![0u8; row_bytes];
    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);

    for y in 0..height as usize {
        let base = y * (row_bytes + 1);
        let filter = raw[base];
        let src = &raw[base + 1..base + 1 + row_bytes];
        unfilter_row(filter, src, &prev_row, &mut curr_row, bpp)?;

        match color_type {
            0 => {
                for &g in &curr_row {
                    rgba.extend_from_slice(&[g, g, g, 255]);
                }
            }
            2 => {
                for &[r, g, b] in curr_row.as_chunks::<3>().0 {
                    rgba.extend_from_slice(&[r, g, b, 255]);
                }
            }
            3 => {
                for &idx in &curr_row {
                    let rgb = palette.get(idx as usize).copied().unwrap_or([0, 0, 0]);
                    let a = trns.get(idx as usize).copied().unwrap_or(255);
                    rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], a]);
                }
            }
            4 => {
                for &[g, a] in curr_row.as_chunks::<2>().0 {
                    rgba.extend_from_slice(&[g, g, g, a]);
                }
            }
            6 => {
                rgba.extend_from_slice(&curr_row);
            }
            _ => unreachable!(),
        }

        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    Ok(RgbaImage {
        width,
        height,
        rgba,
    })
}

fn unfilter_row(
    filter: u8,
    src: &[u8],
    prev: &[u8],
    out: &mut [u8],
    bpp: usize,
) -> Result<(), String> {
    match filter {
        0 => out.copy_from_slice(src),
        1 => {
            for i in 0..src.len() {
                let left = if i >= bpp { out[i - bpp] } else { 0 };
                out[i] = src[i].wrapping_add(left);
            }
        }
        2 => {
            for ((o, &s), &p) in out.iter_mut().zip(src).zip(prev) {
                *o = s.wrapping_add(p);
            }
        }
        3 => {
            for i in 0..src.len() {
                let left = if i >= bpp { out[i - bpp] as u16 } else { 0 };
                let up = prev[i] as u16;
                out[i] = src[i].wrapping_add(((left + up) / 2) as u8);
            }
        }
        4 => {
            for i in 0..src.len() {
                let a = if i >= bpp { out[i - bpp] } else { 0 };
                let b = prev[i];
                let c = if i >= bpp { prev[i - bpp] } else { 0 };
                out[i] = src[i].wrapping_add(paeth_predictor(a, b, c));
            }
        }
        _ => return Err(format!("invalid PNG filter type {filter}")),
    }
    Ok(())
}

fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let a = a as i32;
    let b = b as i32;
    let c = c as i32;
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

/// Decompress a zlib (RFC 1950) stream wrapping raw deflate (RFC 1951).
fn zlib_decompress(input: &[u8], max_out: usize) -> Result<Vec<u8>, String> {
    if input.len() < 6 {
        return Err("zlib stream too short".into());
    }
    let cmf = input[0];
    let flg = input[1];
    if (cmf & 0x0F) != 8 {
        return Err("unsupported zlib compression method".into());
    }
    if !((cmf as u16) * 256 + (flg as u16)).is_multiple_of(31) {
        return Err("invalid zlib header check".into());
    }
    let mut start = 2usize;
    if (flg & 0x20) != 0 {
        start += 4;
    }
    if start >= input.len() {
        return Err("truncated zlib header".into());
    }
    inflate_raw(&input[start..], max_out)
}

struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_buf: u32,
    bit_count: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    fn ensure(&mut self, bits: u8) -> Result<(), String> {
        while self.bit_count < bits {
            if self.byte_pos >= self.data.len() {
                return Err("unexpected end of deflate stream".into());
            }
            self.bit_buf |= (self.data[self.byte_pos] as u32) << self.bit_count;
            self.byte_pos += 1;
            self.bit_count += 8;
        }
        Ok(())
    }

    fn read_bits(&mut self, bits: u8) -> Result<u32, String> {
        if bits == 0 {
            return Ok(0);
        }
        self.ensure(bits)?;
        let val = self.bit_buf & ((1u32 << bits) - 1);
        self.bit_buf >>= bits;
        self.bit_count -= bits;
        Ok(val)
    }

    fn align_byte(&mut self) {
        let drop = self.bit_count % 8;
        self.bit_buf >>= drop;
        self.bit_count -= drop;
    }

    fn read_u16_le_aligned(&mut self) -> Result<u16, String> {
        self.align_byte();
        let lo = self.read_bits(8)? as u16;
        let hi = self.read_bits(8)? as u16;
        Ok(lo | (hi << 8))
    }
}

struct HuffmanTree {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl HuffmanTree {
    fn from_lengths(lengths: &[u8]) -> Result<Self, String> {
        let mut counts = [0u16; 16];
        for &len in lengths {
            if len as usize >= counts.len() {
                return Err("invalid Huffman code length".into());
            }
            if len > 0 {
                counts[len as usize] += 1;
            }
        }
        let mut offsets = [0u16; 16];
        let mut total = 0u16;
        for (off, &cnt) in offsets[1..].iter_mut().zip(&counts[1..]) {
            *off = total;
            total = total.saturating_add(cnt);
        }
        let mut symbols = vec![0u16; total as usize];
        for (sym, &len) in lengths.iter().enumerate() {
            if len > 0 {
                let idx = offsets[len as usize] as usize;
                symbols[idx] = sym as u16;
                offsets[len as usize] += 1;
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, String> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for &count_u16 in &self.counts[1..] {
            code |= reader.read_bits(1)? as i32;
            let count = count_u16 as i32;
            if code - count < first {
                return self
                    .symbols
                    .get((index + (code - first)) as usize)
                    .copied()
                    .ok_or_else(|| "Huffman symbol out of bounds".into());
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("invalid Huffman code".into())
    }
}

fn fixed_trees() -> Result<(HuffmanTree, HuffmanTree), String> {
    let mut lit_lengths = [0u8; 288];
    lit_lengths[..144].fill(8);
    lit_lengths[144..256].fill(9);
    lit_lengths[256..280].fill(7);
    lit_lengths[280..288].fill(8);
    let dist_lengths = [5u8; 32];
    Ok((
        HuffmanTree::from_lengths(&lit_lengths)?,
        HuffmanTree::from_lengths(&dist_lengths)?,
    ))
}

fn dynamic_trees(reader: &mut BitReader<'_>) -> Result<(HuffmanTree, HuffmanTree), String> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let hlit = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;

    let mut code_lengths = [0u8; 19];
    for &slot in &ORDER[..hclen] {
        code_lengths[slot] = reader.read_bits(3)? as u8;
    }
    let code_tree = HuffmanTree::from_lengths(&code_lengths)?;

    let total = hlit + hdist;
    let mut lengths = Vec::with_capacity(total);
    while lengths.len() < total {
        let sym = code_tree.decode(reader)?;
        match sym {
            0..=15 => lengths.push(sym as u8),
            16 => {
                let repeat = reader.read_bits(2)? as usize + 3;
                let prev = *lengths
                    .last()
                    .ok_or_else(|| "repeat code with no previous length".to_string())?;
                for _ in 0..repeat {
                    lengths.push(prev);
                }
            }
            17 => {
                let repeat = reader.read_bits(3)? as usize + 3;
                lengths.resize(lengths.len() + repeat, 0);
            }
            18 => {
                let repeat = reader.read_bits(7)? as usize + 11;
                lengths.resize(lengths.len() + repeat, 0);
            }
            _ => return Err("invalid code-length symbol".into()),
        }
    }
    if lengths.len() != total {
        return Err("Huffman length table overshoot".into());
    }
    Ok((
        HuffmanTree::from_lengths(&lengths[..hlit])?,
        HuffmanTree::from_lengths(&lengths[hlit..])?,
    ))
}

fn inflate_raw(input: &[u8], max_out: usize) -> Result<Vec<u8>, String> {
    const LEN_BASE: [u16; 29] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
        131, 163, 195, 227, 258,
    ];
    const LEN_EXTRA: [u8; 29] = [
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
    ];
    const DIST_BASE: [u16; 30] = [
        1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
        2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
    ];
    const DIST_EXTRA: [u8; 30] = [
        0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
        13, 13,
    ];

    let mut reader = BitReader::new(input);
    let mut out = Vec::with_capacity(max_out.min(65536));

    loop {
        let bfinal = reader.read_bits(1)?;
        let btype = reader.read_bits(2)?;
        match btype {
            0 => {
                let len = reader.read_u16_le_aligned()? as usize;
                let nlen = reader.read_u16_le_aligned()?;
                if (len as u16) != !nlen {
                    return Err("corrupt stored deflate block".into());
                }
                if out.len().saturating_add(len) > max_out {
                    return Err("deflate output exceeds limit".into());
                }
                for _ in 0..len {
                    out.push(reader.read_bits(8)? as u8);
                }
            }
            1 | 2 => {
                let (lit_tree, dist_tree) = if btype == 1 {
                    fixed_trees()?
                } else {
                    dynamic_trees(&mut reader)?
                };
                loop {
                    let sym = lit_tree.decode(&mut reader)?;
                    match sym {
                        0..=255 => {
                            if out.len() >= max_out {
                                return Err("deflate output exceeds limit".into());
                            }
                            out.push(sym as u8);
                        }
                        256 => break,
                        257..=285 => {
                            let idx = (sym - 257) as usize;
                            let length =
                                LEN_BASE[idx] as usize + reader.read_bits(LEN_EXTRA[idx])? as usize;
                            let dist_sym = dist_tree.decode(&mut reader)? as usize;
                            if dist_sym >= DIST_BASE.len() {
                                return Err("invalid distance symbol".into());
                            }
                            let dist = DIST_BASE[dist_sym] as usize
                                + reader.read_bits(DIST_EXTRA[dist_sym])? as usize;
                            if dist == 0 || dist > out.len() {
                                return Err("deflate back-reference out of range".into());
                            }
                            if out.len().saturating_add(length) > max_out {
                                return Err("deflate output exceeds limit".into());
                            }
                            for _ in 0..length {
                                let b = out[out.len() - dist];
                                out.push(b);
                            }
                        }
                        _ => return Err("invalid literal/length symbol".into()),
                    }
                }
            }
            _ => return Err("invalid deflate block type".into()),
        }
        if bfinal != 0 {
            break;
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    fn adler32(data: &[u8]) -> u32 {
        let mut s1 = 1u32;
        let mut s2 = 0u32;
        for &b in data {
            s1 = (s1 + b as u32) % 65521;
            s2 = (s2 + s1) % 65521;
        }
        (s2 << 16) | s1
    }

    fn make_stored_png_rgba(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        let mut raw = Vec::new();
        let row_len = (width as usize) * 4;
        for y in 0..height as usize {
            raw.push(0); // filter = None
            raw.extend_from_slice(&pixels[y * row_len..(y + 1) * row_len]);
        }

        let mut zlib = vec![0x78, 0x01, 0x01]; // CMF, FLG, final stored block
        let len = raw.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(&raw);
        zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

        let mut out = PNG_SIG.to_vec();
        let mut push_chunk = |kind: &[u8; 4], payload: &[u8]| {
            out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(payload);
            let mut crc_buf = Vec::with_capacity(4 + payload.len());
            crc_buf.extend_from_slice(kind);
            crc_buf.extend_from_slice(payload);
            out.extend_from_slice(&crc32(&crc_buf).to_be_bytes());
        };

        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        push_chunk(b"IHDR", &ihdr);
        push_chunk(b"IDAT", &zlib);
        push_chunk(b"IEND", &[]);
        out
    }

    #[test]
    fn decodes_stored_rgba_png() {
        let pixels = vec![
            10, 20, 30, 255, 40, 50, 60, 128, 70, 80, 90, 64, 100, 110, 120, 0,
        ];
        let png = make_stored_png_rgba(2, 2, &pixels);
        let decoded = decode_png(&png).expect("valid PNG");
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.rgba, pixels);
    }

    #[test]
    fn paeth_predictor_matches_spec() {
        assert_eq!(paeth_predictor(10, 20, 15), 15);
        assert_eq!(paeth_predictor(0, 0, 0), 0);
        assert_eq!(paeth_predictor(200, 10, 10), 200);
    }

    #[test]
    fn rejects_corrupt_header() {
        assert!(decode_png(b"not a png").is_err());
    }
}
