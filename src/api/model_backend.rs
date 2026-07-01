use super::{ApiError, ApiResult};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(dead_code)]
pub(super) enum ModelResidency {
    HostMapped,
    DeviceResident,
}

#[derive(Clone)]
#[allow(dead_code)]
pub(super) enum ModelBackend {
    Path,
    #[cfg(feature = "gguf")]
    CandleGguf(Arc<CandleGgufModel>),
}

#[allow(dead_code)]
impl ModelBackend {
    pub(super) fn load(path: &Path) -> ApiResult<Self> {
        if is_gguf(path) {
            return Self::load_gguf(path);
        }
        Ok(Self::Path)
    }

    pub(super) fn residency(&self) -> ModelResidency {
        match self {
            Self::Path => ModelResidency::HostMapped,
            #[cfg(feature = "gguf")]
            Self::CandleGguf(model) => model.residency,
        }
    }

    pub(super) fn kind(&self) -> &'static str {
        match self {
            Self::Path => "path",
            #[cfg(feature = "gguf")]
            Self::CandleGguf(_) => "candle.gguf",
        }
    }

    #[cfg(feature = "gguf")]
    fn load_gguf(path: &Path) -> ApiResult<Self> {
        let device = candle_device()?;
        let var_builder =
            candle_transformers::quantized_var_builder::VarBuilder::from_gguf(path, &device)
                .map_err(|error| {
                    ApiError::InvalidRequest(format!(
                        "failed to load GGUF model with Candle from {}: {error}",
                        path.display()
                    ))
                })?;
        let residency = if matches!(device, candle_core::Device::Cpu) {
            ModelResidency::HostMapped
        } else {
            ModelResidency::DeviceResident
        };
        Ok(Self::CandleGguf(Arc::new(CandleGgufModel {
            device,
            var_builder,
            residency,
        })))
    }

    #[cfg(not(feature = "gguf"))]
    fn load_gguf(path: &Path) -> ApiResult<Self> {
        Err(ApiError::InvalidRequest(format!(
            "model {} is GGUF, but this LightFlow build has no GGUF loader; rebuild with default features or --features gguf",
            path.display()
        )))
    }
}

#[cfg(feature = "gguf")]
pub(super) struct CandleGgufModel {
    #[allow(dead_code)]
    device: candle_core::Device,
    #[allow(dead_code)]
    var_builder: candle_transformers::quantized_var_builder::VarBuilder,
    #[allow(dead_code)]
    residency: ModelResidency,
}

#[cfg(feature = "gguf")]
#[allow(dead_code)]
fn candle_device() -> ApiResult<candle_core::Device> {
    #[cfg(feature = "gguf-cuda")]
    {
        return candle_core::Device::new_cuda(0).map_err(|error| {
            ApiError::InvalidRequest(format!(
                "failed to initialize Candle CUDA device 0: {error}"
            ))
        });
    }

    #[cfg(all(not(feature = "gguf-cuda"), feature = "gguf-metal"))]
    {
        return candle_core::Device::new_metal(0).map_err(|error| {
            ApiError::InvalidRequest(format!(
                "failed to initialize Candle Metal device 0: {error}"
            ))
        });
    }

    #[cfg(not(any(feature = "gguf-cuda", feature = "gguf-metal")))]
    {
        Ok(candle_core::Device::Cpu)
    }
}

#[allow(dead_code)]
fn is_gguf(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
}
