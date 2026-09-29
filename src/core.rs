//! QR Model 2 encoding.
//!
//! This module owns the complete symbol construction pipeline: data bit
//! packing, Reed-Solomon error correction, module placement, mask selection,
//! and format/version metadata. It deliberately exposes a renderer-friendly
//! matrix rather than an image-format-specific value.

use std::fmt;

/// Error-correction levels ordered by their table rows, not by strength.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EcLevel {
    Low = 0,
    Medium = 1,
    Quartile = 2,
    High = 3,
}

impl EcLevel {
    /// Two-bit value stored in the QR format field.
    const fn format_bits(self) -> u8 {
        match self {
            Self::Low => 1,
            Self::Medium => 0,
            Self::Quartile => 3,
            Self::High => 2,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum EncodeError {
    DataTooLong,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataTooLong => formatter.write_str("payload is too long for a QR Code"),
        }
    }
}

impl std::error::Error for EncodeError {}

/// The encoded square module grid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Symbol {
    pub size: usize,
    pub modules: Vec<bool>,
    pub version: u8,
    pub mask: u8,
}

impl Symbol {
    #[inline]
    pub fn module(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.size + x]
    }
}

struct BitBuffer {
    bytes: Vec<u8>,
    bit_len: usize,
}

impl BitBuffer {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            bit_len: 0,
        }
    }

    fn push_bits(&mut self, value: u32, count: usize) {
        debug_assert!(count <= 31 && (count == 31 || value < (1_u32 << count)));
        for shift in (0..count).rev() {
            if self.bit_len.is_multiple_of(8) {
                self.bytes.push(0);
            }
            let bit = ((value >> shift) & 1) as u8;
            let byte_index = self.bit_len / 8;
            self.bytes[byte_index] |= bit << (7 - self.bit_len % 8);
            self.bit_len += 1;
        }
    }
}

/// Encode UTF-8 text using QR byte mode.
///
/// Byte mode is the correct compact representation for the lowercase URLs that
/// motivated this package. Numeric/alphanumeric segmentation will be added as
/// a separately benchmarked optimization; it does not change decoding or the
/// renderer API.
pub fn encode_text(text: &str, level: EcLevel) -> Result<Symbol, EncodeError> {
    let data = text.as_bytes();
    let version = choose_version(data.len(), level).ok_or(EncodeError::DataTooLong)?;
    let data_codewords = make_data_codewords(data, version, level);
    let all_codewords = add_error_correction(&data_codewords, version, level);
    Ok(build_symbol(&all_codewords, version, level))
}

fn choose_version(byte_len: usize, level: EcLevel) -> Option<u8> {
    for version in 1..=40 {
        let count_bits = if version <= 9 { 8 } else { 16 };
        if byte_len >= (1_usize << count_bits) {
            continue;
        }
        let used_bits = 4 + count_bits + byte_len.checked_mul(8)?;
        if used_bits <= data_codewords(version, level) * 8 {
            return Some(version);
        }
    }
    None
}

fn make_data_codewords(data: &[u8], version: u8, level: EcLevel) -> Vec<u8> {
    let capacity = data_codewords(version, level) * 8;
    let mut bits = BitBuffer::new();
    bits.push_bits(0b0100, 4); // Byte mode.
    bits.push_bits(data.len() as u32, if version <= 9 { 8 } else { 16 });
    for byte in data {
        bits.push_bits(u32::from(*byte), 8);
    }

    bits.push_bits(0, (capacity - bits.bit_len).min(4));
    bits.push_bits(0, (8 - bits.bit_len % 8) % 8);

    let mut pad = 0xEC;
    while bits.bytes.len() < capacity / 8 {
        bits.bytes.push(pad);
        pad ^= 0xEC ^ 0x11;
    }
    bits.bytes
}

fn data_codewords(version: u8, level: EcLevel) -> usize {
    raw_data_modules(version) / 8
        - usize::from(ECC_CODEWORDS_PER_BLOCK[level as usize][version as usize])
            * usize::from(NUM_ERROR_CORRECTION_BLOCKS[level as usize][version as usize])
}

/// Number of modules available to codewords after all fixed patterns and
/// metadata areas have been removed. Remainder bits are intentionally included
/// here and discarded by integer division when converting to codewords.
fn raw_data_modules(version: u8) -> usize {
    let version = usize::from(version);
    let mut result = (16 * version + 128) * version + 64;
    if version >= 2 {
        let align = version / 7 + 2;
        result -= (25 * align - 10) * align - 55;
        if version >= 7 {
            result -= 36;
        }
    }
    result
}

fn add_error_correction(data: &[u8], version: u8, level: EcLevel) -> Vec<u8> {
    let block_count = usize::from(NUM_ERROR_CORRECTION_BLOCKS[level as usize][version as usize]);
    let ecc_len = usize::from(ECC_CODEWORDS_PER_BLOCK[level as usize][version as usize]);
    let raw_codewords = raw_data_modules(version) / 8;
    let short_block_len = raw_codewords / block_count;
    let short_block_count = block_count - raw_codewords % block_count;
    let short_data_len = short_block_len - ecc_len;
    let divisor = reed_solomon_divisor(ecc_len);

    let mut blocks: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(block_count);
    let mut offset = 0;
    for index in 0..block_count {
        let length = short_data_len + usize::from(index >= short_block_count);
        let block_data = data[offset..offset + length].to_vec();
        offset += length;
        let ecc = reed_solomon_remainder(&block_data, &divisor);
        blocks.push((block_data, ecc));
    }
    debug_assert_eq!(offset, data.len());

    let mut result = Vec::with_capacity(raw_codewords);
    for index in 0..=short_data_len {
        for (block_data, _) in &blocks {
            if index < block_data.len() {
                result.push(block_data[index]);
            }
        }
    }
    for index in 0..ecc_len {
        for (_, ecc) in &blocks {
            result.push(ecc[index]);
        }
    }
    debug_assert_eq!(result.len(), raw_codewords);
    result
}

fn reed_solomon_divisor(degree: usize) -> Vec<u8> {
    let mut result = vec![0_u8; degree];
    result[degree - 1] = 1;
    let mut root = 1_u8;
    for _ in 0..degree {
        for index in 0..degree {
            result[index] = gf_multiply(result[index], root);
            if index + 1 < degree {
                result[index] ^= result[index + 1];
            }
        }
        root = gf_multiply(root, 0x02);
    }
    result
}

fn reed_solomon_remainder(data: &[u8], divisor: &[u8]) -> Vec<u8> {
    let mut result = vec![0_u8; divisor.len()];
    for byte in data {
        let factor = *byte ^ result[0];
        result.rotate_left(1);
        *result.last_mut().expect("non-empty Reed-Solomon divisor") = 0;
        for (remainder, coefficient) in result.iter_mut().zip(divisor) {
            *remainder ^= gf_multiply(*coefficient, factor);
        }
    }
    result
}

fn gf_multiply(mut left: u8, mut right: u8) -> u8 {
    let mut product = 0_u8;
    for _ in 0..8 {
        product ^= (right & 1).wrapping_neg() & left;
        let carry = left >> 7;
        left = (left << 1) ^ (0x1D & carry.wrapping_neg());
        right >>= 1;
    }
    product
}

struct Matrix {
    size: usize,
    modules: Vec<bool>,
    function: Vec<bool>,
}

impl Matrix {
    fn new(version: u8) -> Self {
        let size = usize::from(version) * 4 + 17;
        Self {
            size,
            modules: vec![false; size * size],
            function: vec![false; size * size],
        }
    }

    #[inline]
    fn index(&self, x: usize, y: usize) -> usize {
        y * self.size + x
    }

    fn set_function(&mut self, x: usize, y: usize, dark: bool) {
        let index = self.index(x, y);
        self.modules[index] = dark;
        self.function[index] = true;
    }

    fn draw_patterns(&mut self, version: u8, level: EcLevel) {
        for index in 0..self.size {
            self.set_function(6, index, index % 2 == 0);
            self.set_function(index, 6, index % 2 == 0);
        }

        self.draw_finder(3, 3);
        self.draw_finder(self.size - 4, 3);
        self.draw_finder(3, self.size - 4);

        let positions = alignment_positions(version);
        let last = positions.len().saturating_sub(1);
        for (x_index, &x) in positions.iter().enumerate() {
            for (y_index, &y) in positions.iter().enumerate() {
                let finder_corner = (x_index == 0 && (y_index == 0 || y_index == last))
                    || (x_index == last && y_index == 0);
                if !finder_corner {
                    self.draw_alignment(x, y);
                }
            }
        }

        self.draw_format(level, 0);
        self.draw_version(version);
    }

    fn draw_finder(&mut self, center_x: usize, center_y: usize) {
        for dy in -4_isize..=4 {
            for dx in -4_isize..=4 {
                let x = center_x as isize + dx;
                let y = center_y as isize + dy;
                if x < 0 || y < 0 || x >= self.size as isize || y >= self.size as isize {
                    continue;
                }
                let distance = dx.unsigned_abs().max(dy.unsigned_abs());
                self.set_function(x as usize, y as usize, distance != 2 && distance != 4);
            }
        }
    }

    fn draw_alignment(&mut self, center_x: usize, center_y: usize) {
        for dy in -2_isize..=2 {
            for dx in -2_isize..=2 {
                let distance = dx.unsigned_abs().max(dy.unsigned_abs());
                self.set_function(
                    (center_x as isize + dx) as usize,
                    (center_y as isize + dy) as usize,
                    distance != 1,
                );
            }
        }
    }

    fn draw_format(&mut self, level: EcLevel, mask: u8) {
        let data = (u32::from(level.format_bits()) << 3) | u32::from(mask);
        let mut remainder = data;
        for _ in 0..10 {
            remainder = (remainder << 1) ^ ((remainder >> 9) * 0x537);
        }
        let bits = ((data << 10) | remainder) ^ 0x5412;
        let bit = |index: usize| ((bits >> index) & 1) != 0;

        for index in 0..=5 {
            self.set_function(8, index, bit(index));
        }
        self.set_function(8, 7, bit(6));
        self.set_function(8, 8, bit(7));
        self.set_function(7, 8, bit(8));
        for index in 9..=14 {
            self.set_function(14 - index, 8, bit(index));
        }

        for index in 0..=7 {
            self.set_function(self.size - 1 - index, 8, bit(index));
        }
        for index in 8..=14 {
            self.set_function(8, self.size - 15 + index, bit(index));
        }
        self.set_function(8, self.size - 8, true);
    }

    fn draw_version(&mut self, version: u8) {
        if version < 7 {
            return;
        }
        let mut remainder = u32::from(version);
        for _ in 0..12 {
            remainder = (remainder << 1) ^ ((remainder >> 11) * 0x1F25);
        }
        let bits = (u32::from(version) << 12) | remainder;
        for index in 0..18 {
            let dark = ((bits >> index) & 1) != 0;
            let a = self.size - 11 + index % 3;
            let b = index / 3;
            self.set_function(a, b, dark);
            self.set_function(b, a, dark);
        }
    }

    fn place_codewords(&mut self, codewords: &[u8]) {
        let mut bit_index = 0;
        let mut right = self.size - 1;
        let mut upward = true;

        while right >= 1 {
            if right == 6 {
                right -= 1;
            }
            for vertical in 0..self.size {
                let y = if upward {
                    self.size - 1 - vertical
                } else {
                    vertical
                };
                for offset in 0..2 {
                    let x = right - offset;
                    let index = self.index(x, y);
                    if self.function[index] {
                        continue;
                    }
                    if bit_index < codewords.len() * 8 {
                        self.modules[index] =
                            ((codewords[bit_index / 8] >> (7 - bit_index % 8)) & 1) != 0;
                        bit_index += 1;
                    }
                }
            }
            upward = !upward;
            if right < 2 {
                break;
            }
            right -= 2;
        }
        debug_assert_eq!(bit_index, codewords.len() * 8);
    }

    fn apply_mask(&mut self, mask: u8) {
        for y in 0..self.size {
            for x in 0..self.size {
                let index = self.index(x, y);
                if !self.function[index] && mask_applies(mask, x, y) {
                    self.modules[index] = !self.modules[index];
                }
            }
        }
    }

    fn penalty(&self) -> u32 {
        let mut result = 0_u32;

        for y in 0..self.size {
            result += line_penalty(
                (0..self.size).map(|x| self.modules[self.index(x, y)]),
                self.size,
            );
        }
        for x in 0..self.size {
            result += line_penalty(
                (0..self.size).map(|y| self.modules[self.index(x, y)]),
                self.size,
            );
        }

        for y in 0..self.size - 1 {
            for x in 0..self.size - 1 {
                let color = self.modules[self.index(x, y)];
                if self.modules[self.index(x + 1, y)] == color
                    && self.modules[self.index(x, y + 1)] == color
                    && self.modules[self.index(x + 1, y + 1)] == color
                {
                    result += 3;
                }
            }
        }

        let dark = self.modules.iter().filter(|module| **module).count();
        let total = self.modules.len();
        let deviation = (dark * 20).abs_diff(total * 10);
        let balance = deviation.div_ceil(total) - 1;
        result + balance as u32 * 10
    }
}

fn build_symbol(codewords: &[u8], version: u8, level: EcLevel) -> Symbol {
    let mut matrix = Matrix::new(version);
    matrix.draw_patterns(version, level);
    matrix.place_codewords(codewords);

    let mut best_mask = 0;
    let mut best_penalty = u32::MAX;
    for mask in 0..8 {
        matrix.apply_mask(mask);
        matrix.draw_format(level, mask);
        let penalty = matrix.penalty();
        if penalty < best_penalty {
            best_penalty = penalty;
            best_mask = mask;
        }
        matrix.apply_mask(mask);
    }
    matrix.apply_mask(best_mask);
    matrix.draw_format(level, best_mask);

    Symbol {
        size: matrix.size,
        modules: matrix.modules,
        version,
        mask: best_mask,
    }
}

fn alignment_positions(version: u8) -> Vec<usize> {
    if version == 1 {
        return Vec::new();
    }
    let count = usize::from(version) / 7 + 2;
    let step = if version == 32 {
        26
    } else {
        (usize::from(version) * 4 + count * 2 + 1) / (count * 2 - 2) * 2
    };
    let mut result = Vec::with_capacity(count);
    result.push(6);
    for index in (0..count - 1).rev() {
        result.push(usize::from(version) * 4 + 10 - index * step);
    }
    result
}

fn mask_applies(mask: u8, x: usize, y: usize) -> bool {
    match mask {
        0 => (x + y).is_multiple_of(2),
        1 => y.is_multiple_of(2),
        2 => x.is_multiple_of(3),
        3 => (x + y).is_multiple_of(3),
        4 => (x / 3 + y / 2).is_multiple_of(2),
        5 => (x * y % 2 + x * y % 3) == 0,
        6 => (x * y % 2 + x * y % 3).is_multiple_of(2),
        7 => ((x + y) % 2 + x * y % 3).is_multiple_of(2),
        _ => unreachable!("mask is constrained to 0 through 7"),
    }
}

fn line_penalty(line: impl Iterator<Item = bool>, size: usize) -> u32 {
    let mut color = false;
    let mut length = 0_usize;
    let mut history = FinderPenalty::new(size);
    let mut result = 0_u32;

    for module in line {
        if module == color {
            length += 1;
            if length == 5 {
                result += 3;
            } else if length > 5 {
                result += 1;
            }
        } else {
            history.add(length);
            if !color {
                result += history.pattern_count() * 40;
            }
            color = module;
            length = 1;
        }
    }

    result + history.terminate_and_count(color, length) * 40
}

/// Run history used by QR penalty rule N3.
///
/// Counting runs instead of matching a fixed 11-module bitmap matters because
/// the 1:1:3:1:1 finder-like ratio can occur at any integer scale. The quiet
/// area outside the symbol is modeled as a light run one symbol-width long.
struct FinderPenalty {
    size: usize,
    runs: [usize; 7],
}

impl FinderPenalty {
    fn new(size: usize) -> Self {
        Self { size, runs: [0; 7] }
    }

    fn add(&mut self, mut length: usize) {
        if self.runs[0] == 0 {
            length += self.size;
        }
        self.runs.copy_within(..6, 1);
        self.runs[0] = length;
    }

    fn pattern_count(&self) -> u32 {
        let unit = self.runs[1];
        let core = unit > 0
            && self.runs[2] == unit
            && self.runs[3] == unit * 3
            && self.runs[4] == unit
            && self.runs[5] == unit;
        u32::from(core && self.runs[0] >= unit * 4 && self.runs[6] >= unit)
            + u32::from(core && self.runs[6] >= unit * 4 && self.runs[0] >= unit)
    }

    fn terminate_and_count(mut self, color: bool, mut length: usize) -> u32 {
        if color {
            self.add(length);
            length = 0;
        }
        length += self.size;
        self.add(length);
        self.pattern_count()
    }
}

// Tables from ISO/IEC 18004. Index zero is unused so the QR version can be
// used directly as the second index.
const ECC_CODEWORDS_PER_BLOCK: [[u8; 41]; 4] = [
    [
        0, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28,
        30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
    [
        0, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28,
        28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
    ],
    [
        0, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30,
        30, 30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
    [
        0, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24,
        30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
];

const NUM_ERROR_CORRECTION_BLOCKS: [[u8; 41]; 4] = [
    [
        0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13,
        14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25,
    ],
    [
        0, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21,
        23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49,
    ],
    [
        0, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27, 29,
        34, 34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68,
    ],
    [
        0, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32,
        35, 37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81,
    ],
];

#[cfg(test)]
mod tests {
    use super::*;
    use qrcodegen::{QrCode, QrCodeEcc, QrSegment, Version};

    #[test]
    fn finite_field_multiplication_matches_reference_values() {
        assert_eq!(gf_multiply(0x57, 0x83), 0x31);
        assert_eq!(gf_multiply(0, 0xA5), 0);
        assert_eq!(gf_multiply(1, 0xA5), 0xA5);
    }

    #[test]
    fn version_one_capacities_match_the_standard() {
        assert_eq!(data_codewords(1, EcLevel::Low), 19);
        assert_eq!(data_codewords(1, EcLevel::Medium), 16);
        assert_eq!(data_codewords(1, EcLevel::Quartile), 13);
        assert_eq!(data_codewords(1, EcLevel::High), 9);
    }

    #[test]
    fn symbol_dimensions_follow_the_version_formula() {
        let symbol = encode_text("https://example.com/a", EcLevel::High).unwrap();
        assert_eq!(symbol.size, usize::from(symbol.version) * 4 + 17);
        assert_eq!(symbol.modules.len(), symbol.size * symbol.size);
        assert!(symbol.mask < 8);
    }

    #[test]
    fn rejects_payload_larger_than_version_forty() {
        assert_eq!(
            encode_text(&"x".repeat(3_000), EcLevel::High),
            Err(EncodeError::DataTooLong)
        );
    }

    #[test]
    fn matrices_match_an_independent_encoder_for_byte_mode() {
        let cases = [
            "",
            "a",
            "https://example.com/test",
            "https://example.com/ābols?日本語=✓",
            &"x".repeat(100),
            &"mixed-1234-ABCD-".repeat(20),
        ];
        let levels = [
            (EcLevel::Low, QrCodeEcc::Low),
            (EcLevel::Medium, QrCodeEcc::Medium),
            (EcLevel::Quartile, QrCodeEcc::Quartile),
            (EcLevel::High, QrCodeEcc::High),
        ];

        for text in cases {
            for (ours_level, reference_level) in levels {
                assert_matches_reference(text, ours_level, reference_level, None);
            }
        }
    }

    #[test]
    fn matrices_match_an_independent_encoder_for_every_version() {
        let levels = [
            (EcLevel::Low, QrCodeEcc::Low),
            (EcLevel::Medium, QrCodeEcc::Medium),
            (EcLevel::Quartile, QrCodeEcc::Quartile),
            (EcLevel::High, QrCodeEcc::High),
        ];

        for (ours_level, reference_level) in levels {
            for version in 1..=40 {
                let bytes = if version == 1 {
                    0
                } else {
                    maximum_byte_length(version - 1, ours_level) + 1
                };
                assert!(bytes <= maximum_byte_length(version, ours_level));
                assert_matches_reference(
                    &"x".repeat(bytes),
                    ours_level,
                    reference_level,
                    Some(version),
                );
            }
        }
    }

    fn maximum_byte_length(version: u8, level: EcLevel) -> usize {
        let count_bits = if version <= 9 { 8 } else { 16 };
        (data_codewords(version, level) * 8 - 4 - count_bits) / 8
    }

    fn assert_matches_reference(
        text: &str,
        ours_level: EcLevel,
        reference_level: QrCodeEcc,
        expected_version: Option<u8>,
    ) {
        let ours = encode_text(text, ours_level).unwrap();
        let segments = [QrSegment::make_bytes(text.as_bytes())];
        let reference = QrCode::encode_segments_advanced(
            &segments,
            reference_level,
            Version::MIN,
            Version::MAX,
            None,
            false,
        )
        .unwrap();
        if let Some(version) = expected_version {
            assert_eq!(
                ours.version, version,
                "unexpected version at {ours_level:?}"
            );
            assert_eq!(reference.version().value(), version);
        }
        assert_eq!(
            ours.size,
            reference.size() as usize,
            "size differs for {} bytes at {ours_level:?}",
            text.len()
        );
        assert_eq!(
            ours.mask,
            reference.mask().value(),
            "mask differs for {} bytes at {ours_level:?}",
            text.len()
        );
        for y in 0..ours.size {
            for x in 0..ours.size {
                assert_eq!(
                    ours.module(x, y),
                    reference.get_module(x as i32, y as i32),
                    "module ({x},{y}) differs for {} bytes at {ours_level:?}",
                    text.len()
                );
            }
        }
    }
}
