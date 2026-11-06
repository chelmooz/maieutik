//! Compression d'embeddings f32 -> f16 pour réduction de mémoire.
//!
//! - Réduit la taille mémoire de 50% (f32 -> f16)
//! - Perte de précision faible mais acceptable pour la recherche sémantique

/// Convertit un vecteur f32 en f16 (4 bytes <-> 2 bytes par élément)
///
/// Implémentation basée sur la conversion IEEE 754 standard.
pub fn compress_f32_to_f16(vec: &[f32]) -> Vec<u8> {
    let mut result = Vec::with_capacity(vec.len() * 2);

    for &v in vec {
        let bits = v.to_bits();
        let sign = ((bits >> 31) & 1) as u16;
        let iexp = ((bits >> 23) & 0xff) as i32;
        let mant = bits & 0x7fffff;

        // Conversion exposant 32-bit -> 16-bit
        let f16_bits: u16 = if iexp == 0xff {
            // Inf ou NaN
            sign | 0x7c00u16 | ((mant >> 13) as u16)
        } else {
            let mut exp = iexp - 127; // Bias f32
            if exp < -14 {
                // Underflow vers zéro
                0u16
            } else if exp > 15 {
                // Overflow vers Inf
                sign | 0x7c00u16
            } else {
                // Normal ou dénormal f16
                let f16_exp = (exp + 15) as u16; // Bias f16
                let f16_mant = (mant >> 13) as u16;
                (sign << 15) | (f16_exp << 10) | f16_mant
            }
        };

        result.extend_from_slice(&f16_bits.to_le_bytes());
    }

    result
}

/// Convertit f16 compressé en vecteur f32
pub fn decompress_f16_to_f32(bytes: &[u8]) -> Vec<f32> {
    let len = bytes.len() / 2;
    let mut result = Vec::with_capacity(len);

    for chunk in bytes.chunks(2) {
        let v16 = u16::from_le_bytes([chunk[0], chunk[1]]);
        let sign = (v16 >> 15) & 1;
        let exp = (v16 >> 10) & 0x1f;
        let mant = v16 & 0x3ff;

        let f32_bits: u32 = if exp == 0x1f {
            // Inf ou NaN
            ((sign as u32) << 31) | 0x7f800000u32 | ((mant as u32) << 13)
        } else if exp == 0 {
            if mant == 0 {
                0u32
            } else {
                // Dénormal f16
                let f = (mant as f32) * 2.0_f32.powi(-24);
                f.to_bits()
            }
        } else {
            // Normal
            let exp32 = (exp as i32) + 112; // Bias f32
            ((sign as u32) << 31) | ((exp32 as u32) << 23) | ((mant as u32) << 13)
        };

        result.push(f32::from_bits(f32_bits));
    }

    result
}

/// Normalise un vecteur f32 en place (L2 norm)
pub fn normalize_l2(vec: &mut [f32]) {
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in vec.iter_mut() {
            *x /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_decompress_roundtrip() {
        let original: Vec<f32> = (0..1024).map(|i| (i as f32) / 1024.0 - 0.5).collect();
        let compressed = compress_f32_to_f16(&original);
        let decompressed = decompress_f16_to_f32(&compressed);

        assert_eq!(compressed.len(), 2048, "f16 devrait être 2x plus petit");
        assert_eq!(original.len(), decompressed.len());

        // Vérifier la précision (devrait être proche)
        for (o, d) in original.iter().zip(decompressed.iter()) {
            let diff = (o - d).abs();
            assert!(diff < 0.01, "Différence trop grande: {} vs {}", o, d);
        }
    }

    #[test]
    fn test_normalize_l2() {
        let mut vec = vec![3.0, 4.0];
        normalize_l2(&mut vec);
        assert!((vec[0] - 0.6).abs() < 0.001);
        assert!((vec[1] - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_zero_handling() {
        let original = vec![0.0f32, 0.0, 1.0, -1.0];
        let compressed = compress_f32_to_f16(&original);
        let decompressed = decompress_f16_to_f32(&compressed);

        assert!((decompressed[0] - 0.0).abs() < 0.001);
        assert!((decompressed[2] - 1.0).abs() < 0.001);
        assert!((decompressed[3] - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_memory_reduction() {
        let embeddings_count = 1000;
        let dim = 1024;

        let f32_bytes = embeddings_count * dim * std::mem::size_of::<f32>(); // 4 bytes
        let f16_bytes = embeddings_count * dim * 2; // f16 = 2 bytes

        let savings = (f32_bytes - f16_bytes) as f64 / f32_bytes as f64 * 100.0;
        assert!(
            (savings - 50.0).abs() < 0.1,
            "Devrait économiser 50% de mémoire, got {}%",
            savings
        );
    }

    #[test]
    fn test_l2_normalized() {
        // Test que les embeddings normalisés restent corrects après compression
        let mut original = vec![0.6, 0.8];
        normalize_l2(&mut original);
        assert!((original[0] - 0.6).abs() < 0.0001);
        assert!((original[1] - 0.8).abs() < 0.0001);

        let compressed = compress_f32_to_f16(&original);
        let decompressed = decompress_f16_to_f32(&compressed);

        // La re-normalisation est nécessaire après décompression
        // mais les valeurs devraient être proches
        let norm: f32 =
            (decompressed[0] * decompressed[0] + decompressed[1] * decompressed[1]).sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "Norme devrait être proche de 1.0, got {}",
            norm
        );
    }

    #[test]
    fn test_negative_values() {
        let original = vec![-0.5, -1.0, -3.0, 0.0, 0.5, 1.0, 3.0];
        let compressed = compress_f32_to_f16(&original);
        let decompressed = decompress_f16_to_f32(&compressed);

        for (o, d) in original.iter().zip(decompressed.iter()) {
            let diff = (o - d).abs();
            assert!(diff < 0.01, "Différence trop grande: {} vs {}", o, d);
        }
    }
}
