//! Raster images for the sheet layer: logos and the like, as plain pixels.

/// A raster image: 8-bit RGB, top row first, with an optional 8-bit alpha channel.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterImage {
    width: u32,
    height: u32,
    /// Three bytes per pixel.
    rgb: Vec<u8>,
    /// One byte per pixel (255 is opaque).
    alpha: Option<Vec<u8>>,
}

/// The largest width or height of an image, in pixels.
pub const MAX_DIMENSION: u32 = 65_535;

/// Pixel data that does not fit the stated size.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ImageError {
    #[error("an image needs a width and a height of at least 1 pixel, got {width} x {height}")]
    Empty { width: u32, height: u32 },
    #[error("an image may be at most {MAX_DIMENSION} pixels wide and high, got {width} x {height}")]
    TooLarge { width: u32, height: u32 },
    #[error("{what} holds {actual} bytes, but a {width} x {height} image needs {expected}")]
    Length {
        what: &'static str,
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
}

impl RasterImage {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Three bytes per pixel, top row first.
    pub fn rgb(&self) -> &[u8] {
        &self.rgb
    }

    pub fn alpha(&self) -> Option<&[u8]> {
        self.alpha.as_deref()
    }

    /// An image of `width` x `height` pixels; the data lengths must match.
    pub fn new(
        width: u32,
        height: u32,
        rgb: Vec<u8>,
        alpha: Option<Vec<u8>>,
    ) -> Result<Self, ImageError> {
        if width == 0 || height == 0 {
            return Err(ImageError::Empty { width, height });
        }
        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(ImageError::TooLarge { width, height });
        }
        let pixels = (width as usize).saturating_mul(height as usize);
        let check = |what: &'static str, expected: usize, actual: usize| {
            if expected == actual {
                Ok(())
            } else {
                Err(ImageError::Length {
                    what,
                    width,
                    height,
                    expected,
                    actual,
                })
            }
        };
        check("the colour data", pixels.saturating_mul(3), rgb.len())?;
        if let Some(alpha) = &alpha {
            check("the alpha data", pixels, alpha.len())?;
        }
        Ok(RasterImage {
            width,
            height,
            rgb,
            alpha,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_matching_lengths() {
        let image = RasterImage::new(2, 1, vec![0; 6], Some(vec![255; 2])).unwrap();
        assert_eq!((image.width(), image.height()), (2, 1));
    }

    #[test]
    fn rejects_zero_dimensions() {
        assert!(matches!(
            RasterImage::new(0, 3, vec![], None),
            Err(ImageError::Empty { .. })
        ));
        assert!(matches!(
            RasterImage::new(3, 0, vec![], None),
            Err(ImageError::Empty { .. })
        ));
    }

    #[test]
    fn rejects_dimensions_above_the_cap() {
        assert!(matches!(
            RasterImage::new(65_536, 1, vec![], None),
            Err(ImageError::TooLarge { .. })
        ));
        assert!(matches!(
            RasterImage::new(1, u32::MAX, vec![], None),
            Err(ImageError::TooLarge { .. })
        ));
    }

    #[test]
    fn rejects_wrong_lengths() {
        assert!(matches!(
            RasterImage::new(2, 2, vec![0; 11], None),
            Err(ImageError::Length {
                what: "the colour data",
                ..
            })
        ));
        assert!(matches!(
            RasterImage::new(2, 2, vec![0; 12], Some(vec![0; 3])),
            Err(ImageError::Length {
                what: "the alpha data",
                ..
            })
        ));
    }
}
