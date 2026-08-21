//! Descriptor types for the images binding

/// How a resized image fits its target box (Cloudflare `fit` semantics)
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResizeFit {
    /// Scale to fit inside the box, keeping the aspect ratio
    Contain,
    /// Scale to fill the box, keeping the aspect ratio, cropping the overflow
    Cover,
    /// Shrink and crop to the box exactly, centered
    Crop,
    /// Scale to fit inside the box, padding the rest with transparency
    Pad,
    /// Like Contain, but never scale up
    ScaleDown,
}

/// Transform applied to an image
///
/// Non-exhaustive: the vocabulary catches up with Cloudflare one transform at
/// a time, in minor versions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ImageTransform {
    /// Resize to the given box (a missing side is derived from the other,
    /// a missing fit means Cloudflare's default, scale-down)
    Resize {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fit: Option<ResizeFit>,
    },
    /// Trim pixels off each edge, the only crop a guest can express: it never
    /// learns the image dimensions, so an absolute rectangle is out of reach
    Trim {
        #[serde(default)]
        top: u32,
        #[serde(default)]
        right: u32,
        #[serde(default)]
        bottom: u32,
        #[serde(default)]
        left: u32,
    },
    /// Rotate clockwise
    Rotate { degrees: u16 },
}

/// Encoding of a transformed image
///
/// Deserializes from the bare token or from the MIME type a Cloudflare worker
/// passes; serialization is always the bare token. Non-exhaustive: formats are
/// added in minor versions.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ImageFormat {
    /// PNG, lossless
    #[serde(alias = "image/png")]
    Png,
    /// JPEG, lossy
    #[serde(alias = "image/jpeg")]
    Jpeg,
    /// WebP, lossy
    #[serde(alias = "image/webp")]
    Webp,
    /// AVIF, lossy
    #[serde(alias = "image/avif")]
    Avif,
}

impl ImageFormat {
    /// MIME type an encoded image of this format is served with
    pub fn content_type(&self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Webp => "image/webp",
            ImageFormat::Avif => "image/avif",
        }
    }
}

/// Output encoding requested for a transform
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct ImageOutput {
    /// Target format
    pub format: ImageFormat,
    /// Encoder quality in 1..=100, ignored for Png
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<u8>,
}

/// Image operation types
///
/// Core only transports the descriptor. Validating it (degrees a multiple of
/// 90, quality within range, input actually decodable) is the handler's job.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImagesOp {
    /// Decode the input, apply the transforms in order, encode as `output`
    Transform {
        input: Vec<u8>,
        transforms: Vec<ImageTransform>,
        output: ImageOutput,
    },
}

/// Result from an image operation
///
/// The image is encoded in the format the caller asked for, so the MIME type
/// comes from [`ImageFormat::content_type`] rather than from the handler.
#[derive(Debug)]
pub enum ImagesResult {
    /// Encoded image
    Image { data: Vec<u8> },
    /// Error message
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::round_trip;

    #[test]
    fn images_op_descriptor_round_trips() {
        let json = concat!(
            r#"{"transform":{"input":[1,2,3],"transforms":["#,
            r#"{"resize":{"width":800,"height":600,"fit":"scale-down"}},"#,
            r#"{"trim":{"top":10,"right":0,"bottom":0,"left":20}},"#,
            r#"{"rotate":{"degrees":90}}"#,
            r#"],"output":{"format":"webp","quality":80}}}"#
        );

        assert_eq!(round_trip::<ImagesOp>(json), json);
    }

    #[test]
    fn image_transform_accepts_a_single_resize_side() {
        let json = r#"{"resize":{"width":800,"fit":"cover"}}"#;

        let transform: ImageTransform = serde_json::from_str(json).unwrap();

        match &transform {
            ImageTransform::Resize { width, height, fit } => {
                assert_eq!(*width, Some(800));
                assert!(height.is_none());
                assert!(matches!(fit, Some(ResizeFit::Cover)));
            }
            other => panic!("expected Resize, got {:?}", other),
        }

        assert_eq!(serde_json::to_string(&transform).unwrap(), json);
    }

    #[test]
    fn image_transform_accepts_a_resize_without_fit() {
        let json = r#"{"resize":{"width":800}}"#;

        let transform: ImageTransform = serde_json::from_str(json).unwrap();

        match &transform {
            ImageTransform::Resize { fit, .. } => assert!(fit.is_none()),
            other => panic!("expected Resize, got {:?}", other),
        }

        assert_eq!(serde_json::to_string(&transform).unwrap(), json);
    }

    #[test]
    fn image_format_accepts_a_mime_type() {
        let json = r#"{"format":"image/webp"}"#;

        let output: ImageOutput = serde_json::from_str(json).unwrap();

        assert!(matches!(output.format, ImageFormat::Webp));
        assert_eq!(
            serde_json::to_string(&output).unwrap(),
            r#"{"format":"webp"}"#
        );
    }

    #[test]
    fn image_formats_have_content_types() {
        assert_eq!(ImageFormat::Png.content_type(), "image/png");
        assert_eq!(ImageFormat::Jpeg.content_type(), "image/jpeg");
        assert_eq!(ImageFormat::Webp.content_type(), "image/webp");
        assert_eq!(ImageFormat::Avif.content_type(), "image/avif");
    }
}
