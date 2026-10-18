//! Thin WebGL2 layer: shader program, mesh upload, canvas textures.

use crate::math::{Mat4, Vec3};
use crate::mesh::{MeshBuilder, FLOATS_PER_VERTEX};
use wasm_bindgen::prelude::*;
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as GL, WebGlProgram, WebGlShader, WebGlTexture,
    WebGlUniformLocation, WebGlVertexArrayObject,
};

const VS: &str = r#"#version 300 es
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_nrm;
layout(location = 2) in vec2 a_uv;
layout(location = 3) in vec3 a_col;
uniform mat4 u_viewProj;
uniform mat4 u_model;
out vec3 v_wpos;
out vec3 v_nrm;
out vec2 v_uv;
out vec3 v_col;
void main() {
    vec4 w = u_model * vec4(a_pos, 1.0);
    v_wpos = w.xyz;
    v_nrm = mat3(u_model) * a_nrm;   // model = rotation + translation only
    v_uv = a_uv;
    v_col = a_col;
    gl_Position = u_viewProj * w;
}
"#;

const FS: &str = r#"#version 300 es
precision highp float;
in vec3 v_wpos;
in vec3 v_nrm;
in vec2 v_uv;
in vec3 v_col;
uniform sampler2D u_tex;
uniform float u_useTex;
uniform float u_unlit;
uniform float u_metal;
uniform float u_shininess;
uniform float u_fogDensity;
uniform vec3 u_eye;
uniform vec3 u_fogColor;
out vec4 outColor;

vec3 toLinear(vec3 c) { return pow(c, vec3(2.2)); }

vec3 light(vec3 n, vec3 v, vec3 l, vec3 lc, vec3 diff, vec3 specCol) {
    float d = max(dot(n, l), 0.0);
    vec3 h = normalize(l + v);
    float s = pow(max(dot(n, h), 0.0), u_shininess) * (u_shininess + 8.0) / 25.0;
    return (diff + specCol * s) * d * lc;
}

void main() {
    vec3 base = toLinear(v_col);
    if (u_useTex > 0.5) {
        base *= toLinear(texture(u_tex, v_uv).rgb);
    }
    vec3 col;
    if (u_unlit > 0.5) {
        col = base;
    } else {
        vec3 n = normalize(v_nrm);
        vec3 v = normalize(u_eye - v_wpos);
        vec3 diff = base * (1.0 - 0.85 * u_metal);
        vec3 specCol = mix(vec3(0.04), base, u_metal);
        col = base * (0.24 + 0.10 * (n.y * 0.5 + 0.5));                 // ambient + sky
        col += light(n, v, normalize(vec3(0.0, 40.0, 20.0)), vec3(1.05), diff, specCol);
        col += light(n, v, normalize(vec3(-20.0, 10.0, -20.0)), toLinear(vec3(1.0, 0.949, 0.816)) * 0.35, diff, specCol);
        col = col / (1.0 + 0.12 * col);                                    // soft shoulder
    }
    float dist = length(u_eye - v_wpos);
    float f = exp(-pow(u_fogDensity * dist, 2.0));
    col = mix(toLinear(u_fogColor), col, f);
    outColor = vec4(pow(col, vec3(1.0 / 2.2)), 1.0);
}
"#;

pub struct Mesh {
    pub vao: WebGlVertexArrayObject,
    pub count: i32,
}

pub struct Uniforms {
    view_proj: WebGlUniformLocation,
    model: WebGlUniformLocation,
    tex: WebGlUniformLocation,
    use_tex: WebGlUniformLocation,
    unlit: WebGlUniformLocation,
    metal: WebGlUniformLocation,
    shininess: WebGlUniformLocation,
    fog_density: WebGlUniformLocation,
    eye: WebGlUniformLocation,
    fog_color: WebGlUniformLocation,
}

pub struct Renderer {
    pub gl: GL,
    program: WebGlProgram,
    u: Uniforms,
    anisotropy: Option<f32>,
    white: WebGlTexture,
}

#[derive(Clone, Copy)]
pub struct Material {
    pub metal: f32,
    pub shininess: f32,
    pub unlit: bool,
}

fn compile(gl: &GL, kind: u32, src: &str) -> Result<WebGlShader, JsValue> {
    let sh = gl.create_shader(kind).ok_or("create_shader failed")?;
    gl.shader_source(&sh, src);
    gl.compile_shader(&sh);
    if gl.get_shader_parameter(&sh, GL::COMPILE_STATUS).as_bool().unwrap_or(false) {
        Ok(sh)
    } else {
        Err(gl.get_shader_info_log(&sh).unwrap_or_default().into())
    }
}

impl Renderer {
    pub fn new(gl: GL) -> Result<Self, JsValue> {
        let vs = compile(&gl, GL::VERTEX_SHADER, VS)?;
        let fs = compile(&gl, GL::FRAGMENT_SHADER, FS)?;
        let program = gl.create_program().ok_or("create_program failed")?;
        gl.attach_shader(&program, &vs);
        gl.attach_shader(&program, &fs);
        gl.link_program(&program);
        if !gl.get_program_parameter(&program, GL::LINK_STATUS).as_bool().unwrap_or(false) {
            return Err(gl.get_program_info_log(&program).unwrap_or_default().into());
        }
        let loc = |name: &str| {
            gl.get_uniform_location(&program, name)
                .ok_or_else(|| JsValue::from_str(&format!("uniform {name} not found")))
        };
        let u = Uniforms {
            view_proj: loc("u_viewProj")?,
            model: loc("u_model")?,
            tex: loc("u_tex")?,
            use_tex: loc("u_useTex")?,
            unlit: loc("u_unlit")?,
            metal: loc("u_metal")?,
            shininess: loc("u_shininess")?,
            fog_density: loc("u_fogDensity")?,
            eye: loc("u_eye")?,
            fog_color: loc("u_fogColor")?,
        };
        // EXT_texture_filter_anisotropic
        let anisotropy = gl
            .get_extension("EXT_texture_filter_anisotropic")
            .ok()
            .flatten()
            .map(|_| {
                gl.get_parameter(0x84FF) // MAX_TEXTURE_MAX_ANISOTROPY_EXT
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0)
                    .min(16.0) as f32
            });
        // 1x1 white texture so the sampler is never unbound
        let white = gl.create_texture().ok_or("create_texture failed")?;
        gl.bind_texture(GL::TEXTURE_2D, Some(&white));
        gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            GL::TEXTURE_2D, 0, GL::RGBA as i32, 1, 1, 0, GL::RGBA, GL::UNSIGNED_BYTE, Some(&[255u8, 255, 255, 255]),
        )?;
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, GL::NEAREST as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::NEAREST as i32);
        gl.enable(GL::DEPTH_TEST);
        gl.disable(GL::CULL_FACE);
        Ok(Renderer { gl, program, u, anisotropy, white })
    }

    pub fn upload(&self, b: &MeshBuilder) -> Result<Mesh, JsValue> {
        let gl = &self.gl;
        let vao = gl.create_vertex_array().ok_or("create_vertex_array failed")?;
        gl.bind_vertex_array(Some(&vao));

        let vbo = gl.create_buffer().ok_or("create_buffer failed")?;
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vbo));
        let verts = js_sys::Float32Array::from(b.vertices.as_slice());
        gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &verts, GL::STATIC_DRAW);

        let ibo = gl.create_buffer().ok_or("create_buffer failed")?;
        gl.bind_buffer(GL::ELEMENT_ARRAY_BUFFER, Some(&ibo));
        let idx = js_sys::Uint32Array::from(b.indices.as_slice());
        gl.buffer_data_with_array_buffer_view(GL::ELEMENT_ARRAY_BUFFER, &idx, GL::STATIC_DRAW);

        let stride = (FLOATS_PER_VERTEX * 4) as i32;
        for (loc, size, offset) in [(0u32, 3, 0), (1, 3, 12), (2, 2, 24), (3, 3, 32)] {
            gl.enable_vertex_attrib_array(loc);
            gl.vertex_attrib_pointer_with_i32(loc, size, GL::FLOAT, false, stride, offset);
        }
        gl.bind_vertex_array(None);
        Ok(Mesh { vao, count: b.indices.len() as i32 })
    }

    /// Creates a texture, or re-uploads into `existing`.
    pub fn canvas_texture(&self, canvas: &HtmlCanvasElement, existing: Option<WebGlTexture>) -> Result<WebGlTexture, JsValue> {
        let gl = &self.gl;
        let tex = match existing {
            Some(t) => t,
            None => gl.create_texture().ok_or("create_texture failed")?,
        };
        gl.bind_texture(GL::TEXTURE_2D, Some(&tex));
        gl.pixel_storei(GL::UNPACK_FLIP_Y_WEBGL, 0); // canvas row 0 → v = 0
        gl.tex_image_2d_with_u32_and_u32_and_html_canvas_element(
            GL::TEXTURE_2D,
            0,
            GL::RGBA as i32,
            GL::RGBA,
            GL::UNSIGNED_BYTE,
            canvas,
        )?;
        gl.generate_mipmap(GL::TEXTURE_2D);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, GL::LINEAR_MIPMAP_LINEAR as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::LINEAR as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, GL::CLAMP_TO_EDGE as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, GL::CLAMP_TO_EDGE as i32);
        if let Some(a) = self.anisotropy {
            gl.tex_parameterf(GL::TEXTURE_2D, 0x84FE, a); // TEXTURE_MAX_ANISOTROPY_EXT
        }
        Ok(tex)
    }

    pub fn begin_frame(&self, w: i32, h: i32, view_proj: &Mat4, eye: Vec3, fog: [f32; 3]) {
        let gl = &self.gl;
        gl.viewport(0, 0, w, h);
        gl.clear_color(fog[0], fog[1], fog[2], 1.0);
        gl.clear(GL::COLOR_BUFFER_BIT | GL::DEPTH_BUFFER_BIT);
        gl.use_program(Some(&self.program));
        gl.uniform_matrix4fv_with_f32_array(Some(&self.u.view_proj), false, view_proj);
        gl.uniform3f(Some(&self.u.eye), eye.x, eye.y, eye.z);
        gl.uniform3f(Some(&self.u.fog_color), fog[0], fog[1], fog[2]);
        gl.uniform1f(Some(&self.u.fog_density), 0.0085);
        gl.uniform1i(Some(&self.u.tex), 0);
    }

    pub fn draw(&self, mesh: &Mesh, model: &Mat4, tex: Option<&WebGlTexture>, m: Material) {
        let gl = &self.gl;
        gl.uniform_matrix4fv_with_f32_array(Some(&self.u.model), false, model);
        gl.uniform1f(Some(&self.u.metal), m.metal);
        gl.uniform1f(Some(&self.u.shininess), m.shininess);
        gl.uniform1f(Some(&self.u.unlit), if m.unlit { 1.0 } else { 0.0 });
        gl.active_texture(GL::TEXTURE0);
        match tex {
            Some(t) => {
                gl.bind_texture(GL::TEXTURE_2D, Some(t));
                gl.uniform1f(Some(&self.u.use_tex), 1.0);
            }
            None => {
                gl.bind_texture(GL::TEXTURE_2D, Some(&self.white));
                gl.uniform1f(Some(&self.u.use_tex), 0.0);
            }
        }
        gl.bind_vertex_array(Some(&mesh.vao));
        gl.draw_elements_with_i32(GL::TRIANGLES, mesh.count, GL::UNSIGNED_INT, 0);
        gl.bind_vertex_array(None);
    }
}
