use std::fmt;

/// Geometry payload encodings accepted by ExtremEngine's asset pipeline.
///
/// The enum names the wire representation only. It does not claim that every
/// build has a decoder for every codec.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GeometryCodec {
    RawGltf,
    Meshopt,
    Draco,
}

impl GeometryCodec {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RawGltf => "raw-gltf",
            Self::Meshopt => "meshopt",
            Self::Draco => "draco",
        }
    }
}

/// Decoder failure with no backend-specific authority over the asset registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GeometryDecodeError {
    UnsupportedCodec(GeometryCodec),
    InvalidPayload(String),
}

impl fmt::Display for GeometryDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCodec(codec) => {
                write!(formatter, "geometry codec is not supported: {}", codec.as_str())
            }
            Self::InvalidPayload(detail) => write!(formatter, "invalid geometry payload: {detail}"),
        }
    }
}

impl std::error::Error for GeometryDecodeError {}

/// Stable boundary between asset selection and codec implementations.
///
/// Implementations may wrap standards-compliant external libraries. Callers
/// must select the codec explicitly; implementations must not guess a codec
/// from untrusted payload bytes.
pub trait GeometryDecoder {
    type Output;

    fn supports(&self, codec: GeometryCodec) -> bool;

    fn decode(
        &self,
        codec: GeometryCodec,
        payload: &[u8],
    ) -> Result<Self::Output, GeometryDecodeError>;
}

/// Small helper that enforces capability checking before decoding.
pub fn decode_with<D: GeometryDecoder>(
    decoder: &D,
    codec: GeometryCodec,
    payload: &[u8],
) -> Result<D::Output, GeometryDecodeError> {
    if !decoder.supports(codec) {
        return Err(GeometryDecodeError::UnsupportedCodec(codec));
    }
    decoder.decode(codec, payload)
}

#[cfg(test)]
mod tests {
    use super::{GeometryCodec, GeometryDecodeError, GeometryDecoder, decode_with};

    struct RawOnly;

    impl GeometryDecoder for RawOnly {
        type Output = Vec<u8>;

        fn supports(&self, codec: GeometryCodec) -> bool {
            codec == GeometryCodec::RawGltf
        }

        fn decode(
            &self,
            codec: GeometryCodec,
            payload: &[u8],
        ) -> Result<Self::Output, GeometryDecodeError> {
            if codec != GeometryCodec::RawGltf {
                return Err(GeometryDecodeError::UnsupportedCodec(codec));
            }
            if payload.is_empty() {
                return Err(GeometryDecodeError::InvalidPayload("empty payload".to_owned()));
            }
            Ok(payload.to_vec())
        }
    }

    #[test]
    fn codec_names_are_stable() {
        assert_eq!(GeometryCodec::RawGltf.as_str(), "raw-gltf");
        assert_eq!(GeometryCodec::Meshopt.as_str(), "meshopt");
        assert_eq!(GeometryCodec::Draco.as_str(), "draco");
    }

    #[test]
    fn unsupported_codec_fails_before_decode() {
        let result = decode_with(&RawOnly, GeometryCodec::Draco, b"payload");
        assert_eq!(
            result,
            Err(GeometryDecodeError::UnsupportedCodec(GeometryCodec::Draco))
        );
    }

    #[test]
    fn supported_codec_delegates_to_decoder() {
        assert_eq!(
            decode_with(&RawOnly, GeometryCodec::RawGltf, b"glTF"),
            Ok(b"glTF".to_vec())
        );
    }

    #[test]
    fn decoder_errors_are_preserved() {
        assert_eq!(
            decode_with(&RawOnly, GeometryCodec::RawGltf, b""),
            Err(GeometryDecodeError::InvalidPayload("empty payload".to_owned()))
        );
    }
}
