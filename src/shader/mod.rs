//! Shader validation against the VZGLYD contract.
//!
//! This module validates WGSL shaders against the VZGLYD shader contract,
//! ensuring they conform to the expected interface and binding layout.

use std::fmt;

use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::{Module, ShaderStage};

/// Shader contract type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderContract {
    /// Screen-space 2D shaders.
    Screen2D,
    /// World-space 3D shaders.
    World3D,
}

/// Shader prelude for Screen2D contracts.
const SCREEN2D_SHADER_PRELUDE: &str = r#"// VZGLYD shader contract v1: Screen2D
const VZGLYD_SHADER_CONTRACT_VERSION: u32 = 1u;

struct VzglydVertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) mode: f32,
};

struct VzglydVertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) mode: f32,
};

struct VzglydUniforms {
    time: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var t_diffuse: texture_2d<f32>;
@group(0) @binding(1) var t_font: texture_2d<f32>;
@group(0) @binding(2) var t_detail: texture_2d<f32>;
@group(0) @binding(3) var t_lookup: texture_2d<f32>;
@group(0) @binding(4) var s_diffuse: sampler;
@group(0) @binding(5) var s_font: sampler;
@group(0) @binding(6) var<uniform> u: VzglydUniforms;
"#;

/// Shader prelude for World3D contracts.
const WORLD3D_SHADER_PRELUDE: &str = r#"// VZGLYD shader contract v1: World3D
const VZGLYD_SHADER_CONTRACT_VERSION: u32 = 1u;

struct VzglydVertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) mode: f32,
};

struct VzglydVertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) mode: f32,
};

struct VzglydUniforms {
    view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,
    time: f32,
    fog_color: vec4<f32>,
    fog_start: f32,
    fog_end: f32,
    clock_seconds: f32,
    _pad: f32,
    ambient_light: vec4<f32>,
    main_light_dir: vec4<f32>,
    main_light_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: VzglydUniforms;
@group(0) @binding(1) var t_font: texture_2d<f32>;
@group(0) @binding(2) var t_noise: texture_2d<f32>;
@group(0) @binding(3) var t_material_a: texture_2d<f32>;
@group(0) @binding(4) var t_material_b: texture_2d<f32>;
@group(0) @binding(5) var s_clamp: sampler;
@group(0) @binding(6) var s_repeat: sampler;
"#;

/// Returns the shader prelude for the given contract.
pub fn shader_prelude(contract: ShaderContract) -> &'static str {
    match contract {
        ShaderContract::Screen2D => SCREEN2D_SHADER_PRELUDE,
        ShaderContract::World3D => WORLD3D_SHADER_PRELUDE,
    }
}

/// Assembles a complete shader by prepending the contract prelude.
pub fn assembled_slide_shader_source(contract: ShaderContract, shader_body: &str) -> String {
    format!("{}\n{}", shader_prelude(contract), shader_body)
}

/// Shader validation error.
#[derive(Debug)]
pub struct ShaderValidationError {
    summary: String,
    diagnostic: String,
}

impl ShaderValidationError {
    /// Returns a brief summary of the error.
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Returns the full diagnostic output with source code.
    pub fn diagnostic(&self) -> &str {
        &self.diagnostic
    }

    fn from_parse(label: &str, source: &str, error: naga::front::wgsl::ParseError) -> Self {
        let summary = error.to_string();
        let diagnostic = error.emit_to_string_with_path(source, label);
        Self { summary, diagnostic }
    }

    fn from_naga_validation(
        label: &str,
        source: &str,
        error: naga::WithSpan<naga::valid::ValidationError>,
    ) -> Self {
        let summary = error.to_string();
        let diagnostic = naga::error::ShaderError {
            source: source.to_string(),
            label: Some(label.to_string()),
            inner: Box::new(error),
        }
        .to_string();
        Self { summary, diagnostic }
    }

    fn custom(summary: impl Into<String>, diagnostic: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            diagnostic: diagnostic.into(),
        }
    }
}

impl fmt::Display for ShaderValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.summary)
    }
}

impl std::error::Error for ShaderValidationError {}

/// Validates a complete shader source (prelude + body) against the contract.
///
/// # Arguments
/// * `label` - Label for error reporting (e.g., file path)
/// * `source` - Complete shader source (including prelude)
/// * `contract` - The contract to validate against
/// * `vs_entry` - Vertex shader entry point name
/// * `fs_entry` - Fragment shader entry point name
///
/// # Returns
/// * `Ok(())` if validation passes
/// * `Err(ShaderValidationError)` if validation fails
pub fn validate_shader_source(
    label: &str,
    source: &str,
    contract: ShaderContract,
    vs_entry: &str,
    fs_entry: &str,
) -> Result<(), ShaderValidationError> {
    // Parse the shader
    let module = naga::front::wgsl::parse_str(source)
        .map_err(|error| ShaderValidationError::from_parse(label, source, error))?;

    // Validate with naga
    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
    validator
        .validate(&module)
        .map_err(|error| ShaderValidationError::from_naga_validation(label, source, error))?;

    // Reject unsupported features
    reject_unsupported_features(&module)?;

    // Validate entry points exist
    validate_entry_points(&module, vs_entry, fs_entry)?;

    // Validate contract-specific requirements
    validate_contract_bindings(&module, contract)?;

    Ok(())
}

/// Validates a shader body (without prelude) by prepending the contract prelude.
///
/// This is the main entry point for validating slide shaders.
///
/// # Arguments
/// * `label` - Label for error reporting
/// * `shader_body` - Shader body (without prelude)
/// * `contract` - The contract to validate against
/// * `vs_entry` - Vertex shader entry point
/// * `fs_entry` - Fragment shader entry point
///
/// # Returns
/// * `Ok(String)` with the complete shader source if validation passes
/// * `Err(ShaderValidationError)` if validation fails
pub fn validate_slide_shader_body(
    label: &str,
    shader_body: &str,
    contract: ShaderContract,
    vs_entry: &str,
    fs_entry: &str,
) -> Result<String, ShaderValidationError> {
    let shader_source = assembled_slide_shader_source(contract, shader_body);
    validate_shader_source(label, &shader_source, contract, vs_entry, fs_entry)?;
    Ok(shader_source)
}

/// Rejects unsupported shader features.
fn reject_unsupported_features(module: &Module) -> Result<(), ShaderValidationError> {
    // Reject compute shaders
    if let Some(entry_point) = module
        .entry_points
        .iter()
        .find(|entry_point| entry_point.stage == ShaderStage::Compute)
    {
        return Err(ShaderValidationError::custom(
            format!(
                "compute entry point '{}' is not supported in slide shaders",
                entry_point.name
            ),
            "Compute shaders are not supported. Use vertex and fragment shaders only.".to_string(),
        ));
    }

    // Reject storage buffers and push constants
    for (_, global) in module.global_variables.iter() {
        match global.space {
            naga::AddressSpace::Storage { .. } => {
                return Err(ShaderValidationError::custom(
                    "storage buffers are not supported in slide shaders".to_string(),
                    "Use uniform buffers or textures instead.".to_string(),
                ));
            }
            naga::AddressSpace::PushConstant => {
                return Err(ShaderValidationError::custom(
                    "push constants are not supported in slide shaders".to_string(),
                    "Use uniform buffers instead.".to_string(),
                ));
            }
            _ => {}
        }
    }

    Ok(())
}

/// Validates that required entry points exist.
fn validate_entry_points(
    module: &Module,
    vs_entry: &str,
    fs_entry: &str,
) -> Result<(), ShaderValidationError> {
    let mut has_vertex = false;
    let mut has_fragment = false;

    for entry_point in &module.entry_points {
        if entry_point.stage == ShaderStage::Vertex && entry_point.name == vs_entry {
            has_vertex = true;
        }
        if entry_point.stage == ShaderStage::Fragment && entry_point.name == fs_entry {
            has_fragment = true;
        }
    }

    if !has_vertex {
        return Err(ShaderValidationError::custom(
            format!("missing vertex entry point '{}'", vs_entry),
            "The shader must export a vertex function with the specified name.".to_string(),
        ));
    }

    if !has_fragment {
        return Err(ShaderValidationError::custom(
            format!("missing fragment entry point '{}'", fs_entry),
            "The shader must export a fragment function with the specified name.".to_string(),
        ));
    }

    Ok(())
}

/// Validates contract-specific binding requirements.
fn validate_contract_bindings(
    module: &Module,
    _contract: ShaderContract,
) -> Result<(), ShaderValidationError> {
    // Validate that no bind groups other than 0 are used
    for (_, global) in module.global_variables.iter() {
        if let Some(binding) = &global.binding {
            if binding.group != 0 {
                return Err(ShaderValidationError::custom(
                    format!(
                        "binding @group({}) is unsupported; slide shaders may only use bind group 0",
                        binding.group
                    ),
                    "Change the bind group to 0.".to_string(),
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_minimal_vertex_fragment() {
        let shader_body = r#"
@vertex
fn vs_main(input: VzglydVertexInput) -> VzglydVertexOutput {
    var output: VzglydVertexOutput;
    output.clip_pos = vec4<f32>(input.position, 1.0);
    output.tex_coords = input.tex_coords;
    output.color = input.color;
    output.mode = input.mode;
    return output;
}

@fragment
fn fs_main(input: VzglydVertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

        let result = validate_slide_shader_body(
            "test.wgsl",
            shader_body,
            ShaderContract::Screen2D,
            "vs_main",
            "fs_main",
        );
        assert!(result.is_ok(), "Validation failed: {:?}", result);
    }

    #[test]
    fn reject_compute_shader() {
        let shader_body = r#"
@compute @workgroup_size(1)
fn cs_main() {}
"#;

        let result = validate_slide_shader_body(
            "test.wgsl",
            shader_body,
            ShaderContract::Screen2D,
            "vs_main",
            "fs_main",
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.summary().contains("compute"));
    }

    #[test]
    fn reject_storage_buffer() {
        let shader_body = r#"
@group(0) @binding(10) var<storage> data: array<f32>;

@vertex
fn vs_main(input: VzglydVertexInput) -> VzglydVertexOutput {
    var output: VzglydVertexOutput;
    output.clip_pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    output.tex_coords = vec2<f32>(0.0, 0.0);
    output.color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    output.mode = 0.0;
    return output;
}

@fragment
fn fs_main(input: VzglydVertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

        let result = validate_slide_shader_body(
            "test.wgsl",
            shader_body,
            ShaderContract::Screen2D,
            "vs_main",
            "fs_main",
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.summary().contains("storage"));
    }
}
