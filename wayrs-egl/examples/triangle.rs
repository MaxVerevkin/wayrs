use std::collections::HashMap;

use gles31::*;

use wayrs_client::global::*;
use wayrs_client::protocol::*;
use wayrs_client::EventCtx;
use wayrs_client::{Connection, IoMode};
use wayrs_egl::*;
use wayrs_protocols::linux_dmabuf_unstable_v1::*;
use wayrs_protocols::xdg_shell::*;
use wayrs_utils::dmabuf_feedback::*;

// Two buffers is not enough for smooth resizing, at least on my system. Three seems to be enough.
const BUFFERS: usize = 3;

struct Renderer {
    format: Fourcc,
    modifiers: Vec<u64>,
    buffers: BufferPool<BUFFERS>,
    rbo: u32,
    screensize_loc: i32,
    time_loc: i32,

    #[allow(dead_code)]
    egl_context: EglContext,
    egl_display: EglDisplay,
}

impl Renderer {
    pub fn new(egl_display: EglDisplay, format: Fourcc, modifiers: Vec<u64>) -> Self {
        println!(
            "EGL v{}.{}",
            egl_display.major_version(),
            egl_display.minor_version()
        );

        let egl_context = EglContextBuilder::new(GraphicsApi::OpenGlEs)
            .version(3, 1)
            .build(&egl_display)
            .unwrap();

        egl_context.make_current().unwrap();

        unsafe { load_gl_functions(&|name| egl_ffi::eglGetProcAddress(name as *const _)).unwrap() };

        let shader_prog;
        let mut vbo = 0;
        let mut vao = 0;
        let screensize_loc;
        let time_loc;
        let mut fb = 0;
        let mut rbo = 0;

        unsafe {
            {
                let mut major = 0;
                let mut minor = 0;
                glGetIntegerv(GL_MAJOR_VERSION, &mut major);
                glGetIntegerv(GL_MINOR_VERSION, &mut minor);
                println!("OpenGL-ES v{major}.{minor}");
            }

            glGenFramebuffers(1, &mut fb);
            glGenRenderbuffers(1, &mut rbo);

            glBindFramebuffer(GL_FRAMEBUFFER, fb);
            glBindRenderbuffer(GL_RENDERBUFFER, rbo);
            glDrawBuffers(1, &GL_COLOR_ATTACHMENT0);

            let vertex_shader = b"
#version 310 es
precision mediump float;
in vec3 vp;
uniform vec2 screensize;
uniform float t;
void main() {
    float ratio = screensize.x / screensize.y;
    gl_Position = vec4(vp, 1.0);
    if (ratio > 1.0)
        gl_Position.x /= ratio;
    else
        gl_Position.y *= ratio;
    gl_Position.x *= 0.8 + sin(t) * 0.2;
    gl_Position.y *= 0.8 + sin(t) * 0.2;
}\0";

            let fragment_shader = b"
#version 310 es
precision mediump float;
out vec4 frag_color;
uniform vec2 screensize;
uniform float t;
void main() {
    vec2 uv = gl_FragCoord.xy / screensize * 2.0 - 1.0;
    float ratio = screensize.x / screensize.y;
    if (ratio > 1.0)
        uv.x *= ratio;
    else
        uv.y /= ratio;
    vec3 col = 0.5 + 0.5*cos(t*1.2 + 2.0*uv.xyx+vec3(0.0,3.0,4.0));
    frag_color = vec4(col, 1.0);
}\0";

            let vs = glCreateShader(GL_VERTEX_SHADER);
            glShaderSource(vs, 1, &(vertex_shader.as_ptr() as _), std::ptr::null());
            glCompileShader(vs);
            assert_shader_ok(vs);

            let fs = glCreateShader(GL_FRAGMENT_SHADER);
            glShaderSource(fs, 1, &(fragment_shader.as_ptr() as _), std::ptr::null());
            glCompileShader(fs);
            assert_shader_ok(fs);

            shader_prog = glCreateProgram();
            glAttachShader(shader_prog, fs);
            glAttachShader(shader_prog, vs);
            glLinkProgram(shader_prog);
            assert_shader_program_ok(shader_prog);

            glUseProgram(shader_prog);

            screensize_loc = glGetUniformLocation(shader_prog, b"screensize\0".as_ptr().cast());
            time_loc = glGetUniformLocation(shader_prog, b"t\0".as_ptr().cast());

            glDeleteShader(vs);
            glDeleteShader(fs);

            glGenBuffers(1, &mut vbo);
            glBindBuffer(GL_ARRAY_BUFFER, vbo);
            use std::f32::consts::FRAC_PI_3;
            let points: [f32; 9] = [
                f32::sin(0.0),
                f32::cos(0.0),
                0.0,
                f32::sin(FRAC_PI_3 * -2.0),
                f32::cos(FRAC_PI_3 * -2.0),
                0.0,
                f32::sin(FRAC_PI_3 * -4.0),
                f32::cos(FRAC_PI_3 * -4.0),
                0.0,
            ];
            glBufferData(
                GL_ARRAY_BUFFER,
                std::mem::size_of_val(&points) as _,
                points.as_ptr() as *const _,
                GL_STATIC_DRAW,
            );

            glGenVertexArrays(1, &mut vao);
            glBindVertexArray(vao);
            glEnableVertexAttribArray(0);
            glBindBuffer(GL_ARRAY_BUFFER, vbo);
            glVertexAttribPointer(0, 3, GL_FLOAT, 0, 0, std::ptr::null());
        }

        Self {
            format,
            modifiers,
            buffers: BufferPool::new(),
            rbo,
            screensize_loc,
            time_loc,

            egl_context,
            egl_display,
        }
    }

    pub fn render(
        &mut self,
        conn: &mut Connection<State>,
        width: u32,
        height: u32,
        time: f32,
    ) -> Option<&Buffer> {
        let buf = self
            .buffers
            .get_buffer(
                &self.egl_display,
                conn,
                width,
                height,
                self.format,
                &self.modifiers,
            )
            .unwrap()?;

        unsafe {
            glUniform2f(self.screensize_loc, width as f32, height as f32);
            glUniform1f(self.time_loc, time);

            buf.set_as_gl_renderbuffer_storage();
            glFramebufferRenderbuffer(
                GL_FRAMEBUFFER,
                GL_COLOR_ATTACHMENT0,
                GL_RENDERBUFFER,
                self.rbo,
            );

            assert_eq!(
                glCheckFramebufferStatus(GL_FRAMEBUFFER),
                GL_FRAMEBUFFER_COMPLETE,
                "framebuffer incomplete"
            );

            glViewport(0, 0, width as _, height as _);
            glClearColor(0.0, 0.0, 0.0, 0.6);
            glClear(GL_COLOR_BUFFER_BIT);
            glDrawArrays(GL_TRIANGLES, 0, 3);

            glFinish();
        };

        Some(buf)
    }
}

fn main() {
    let (mut conn, globals) = Connection::connect_and_collect_globals().unwrap();
    let linux_dmabuf: ZwpLinuxDmabufV1 = globals.bind(&mut conn, 2..).unwrap();
    let wl_compositor: WlCompositor = globals.bind(&mut conn, ..).unwrap();
    let xdg_wm_base: XdgWmBase = globals.bind_with_cb(&mut conn, .., xdg_wm_base_cb).unwrap();

    let mut state = State {
        time: 0.0,
        time_anchor: None,
        surf: Surface::new(&mut conn, wl_compositor, xdg_wm_base, linux_dmabuf),
        linux_dmabuf,
        gl: None,
    };

    while !state.surf.should_close {
        conn.flush(IoMode::Blocking).unwrap();
        conn.recv_events(IoMode::Blocking).unwrap();
        conn.dispatch_events(&mut state);
    }
}

struct State {
    time: f32,
    time_anchor: Option<u32>,
    surf: Surface,
    linux_dmabuf: ZwpLinuxDmabufV1,
    gl: Option<Renderer>,
}

impl State {
    fn render(&mut self, conn: &mut Connection<State>, time: Option<u32>) {
        if !self.surf.mapped || self.surf.frame_cb.is_some() {
            return;
        }
        let Some(gl) = &mut self.gl else { return };

        if let Some(time) = time {
            let time_anchor = *self.time_anchor.get_or_insert(time);
            self.time = (time - time_anchor) as f32 / 700.0;
        }

        if let Some(buf) = gl.render(conn, self.surf.width, self.surf.height, self.time) {
            let wl_buffer = unsafe { buf.wl_buffer() };
            self.surf.wl.attach(conn, Some(wl_buffer), 0, 0);
            self.surf.wl.damage(conn, 0, 0, i32::MAX, i32::MAX);
        } else {
            eprintln!("skipping frame (not enough buffers)");
        }

        self.surf.frame_cb = Some(self.surf.wl.frame_with_cb(conn, |ctx| {
            let wl_callback::Event::Done(time) = ctx.event else {
                unreachable!()
            };
            assert_eq!(ctx.state.surf.frame_cb, Some(ctx.proxy));
            ctx.state.surf.frame_cb = None;
            ctx.state.render(ctx.conn, Some(time));
        }));

        self.surf.wl.commit(conn);
    }
}

struct Surface {
    wl: WlSurface,
    #[allow(dead_code)]
    xdg_surface: XdgSurface,
    #[allow(dead_code)]
    xdg_toplevel: XdgToplevel,
    dmabuf_feedback: DmabufFeedback,
    width: u32,
    height: u32,
    frame_cb: Option<WlCallback>,
    mapped: bool,
    should_close: bool,
}

impl Surface {
    fn new(
        conn: &mut Connection<State>,
        wl_compositor: WlCompositor,
        xdg_wm_base: XdgWmBase,
        linux_dmabuf: ZwpLinuxDmabufV1,
    ) -> Self {
        let wl = wl_compositor.create_surface(conn);
        let dmabuf_feedback = DmabufFeedback::get_for_surface(conn, linux_dmabuf, wl);
        let xdg_surface = xdg_wm_base.get_xdg_surface(conn, wl);
        let xdg_toplevel = xdg_surface.get_toplevel(conn);

        // DMABUFs have origin at top-left corner, but OpenGL has origin at bottom-left. This
        // results in a y-flipped image.
        wl.set_buffer_transform(conn, wl_output::Transform::Flipped180);

        conn.set_callback_for(xdg_surface, |ctx| {
            if let xdg_surface::Event::Configure(serial) = ctx.event {
                ctx.proxy.ack_configure(ctx.conn, serial);
                ctx.state.surf.mapped = true;
                ctx.state.render(ctx.conn, None);
            }
        });

        conn.set_callback_for(xdg_toplevel, |ctx| match ctx.event {
            xdg_toplevel::Event::Configure(args) => {
                if args.width != 0 {
                    ctx.state.surf.width = args.width.try_into().unwrap();
                }
                if args.height != 0 {
                    ctx.state.surf.height = args.height.try_into().unwrap();
                }
            }
            xdg_toplevel::Event::Close => {
                ctx.state.surf.should_close = true;
                ctx.conn.break_dispatch_loop();
            }
            _ => (),
        });

        xdg_toplevel.set_app_id(conn, wayrs_client::cstr!("wayrs-egl").into());
        xdg_toplevel.set_title(conn, wayrs_client::cstr!("TITLE").into());

        wl.commit(conn);

        Self {
            wl,
            xdg_surface,
            xdg_toplevel,
            dmabuf_feedback,
            width: 500,
            height: 500,
            frame_cb: None,
            mapped: false,
            should_close: false,
        }
    }
}

impl DmabufFeedbackHandler for State {
    fn get_dmabuf_feedback(&mut self, wl: ZwpLinuxDmabufFeedbackV1) -> &mut DmabufFeedback {
        assert_eq!(wl, self.surf.dmabuf_feedback.wl());
        &mut self.surf.dmabuf_feedback
    }

    fn feedback_done(&mut self, _: &mut Connection<Self>, wl: ZwpLinuxDmabufFeedbackV1) {
        assert_eq!(wl, self.surf.dmabuf_feedback.wl());

        if self.gl.is_some() {
            eprintln!("only initial dmabuf feedback is implemented");
            return;
        }

        let main_dev = self
            .surf
            .dmabuf_feedback
            .main_device()
            .expect("dmabuf_feedback: main_device not advertised");

        let drm_device =
            DrmDevice::new_from_id(main_dev).expect("could not create drm_device from id");

        let render_node = drm_device
            .render_node()
            .expect("drm_device does not have a render node");

        let egl_display = EglDisplay::new(self.linux_dmabuf, render_node).unwrap();

        let format_table = self.surf.dmabuf_feedback.format_table();
        let mut formats = HashMap::<Fourcc, Vec<u64>>::new();

        for tranche in self.surf.dmabuf_feedback.tranches() {
            if tranche
                .flags
                .contains(zwp_linux_dmabuf_feedback_v1::TrancheFlags::Scanout)
            {
                continue;
            }
            for &index in tranche.formats.as_ref().expect("tranche.formats") {
                let fmt = format_table[index as usize];
                if egl_display.is_format_supported(Fourcc(fmt.fourcc), fmt.modifier) {
                    formats
                        .entry(Fourcc(fmt.fourcc))
                        .or_default()
                        .push(fmt.modifier);
                }
            }
        }

        // prefer DRM_FORMAT_ARGB8888, fallback to anything
        const DRM_FORMAT_ARGB8888: Fourcc = Fourcc(u32::from_le_bytes(*b"AR24"));
        let (format, mods) = match formats.remove(&DRM_FORMAT_ARGB8888) {
            Some(mods) => (DRM_FORMAT_ARGB8888, mods),
            None => {
                let (format, mods) = formats
                    .into_iter()
                    .next()
                    .expect("at least one supported format");
                eprintln!("ARGB8888 not supported, falling back to {format:?}");
                (format, mods)
            }
        };

        self.gl = Some(Renderer::new(egl_display, format, mods));
    }
}

fn xdg_wm_base_cb(ctx: EventCtx<State, XdgWmBase>) {
    if let xdg_wm_base::Event::Ping(serial) = ctx.event {
        ctx.proxy.pong(ctx.conn, serial);
    }
}

fn assert_shader_ok(shader: u32) {
    let mut success = 0;
    unsafe {
        glGetShaderiv(shader, GL_COMPILE_STATUS, &mut success);
    }

    if success != 1 {
        let mut log = [0u8; 1024];
        let mut len = 0;
        unsafe {
            glGetShaderInfoLog(shader, log.len() as _, &mut len, log.as_mut_ptr() as *mut _);
        }
        let msg = std::str::from_utf8(&log[..len as usize]).unwrap();
        panic!("Shader error:\n{msg}");
    }
}

fn assert_shader_program_ok(shader_program: u32) {
    let mut success = 0;
    unsafe {
        glGetProgramiv(shader_program, GL_LINK_STATUS, &mut success);
    }

    if success != 1 {
        let mut log = [0u8; 1024];
        let mut len = 0;
        unsafe {
            glGetProgramInfoLog(
                shader_program,
                log.len() as _,
                &mut len,
                log.as_mut_ptr() as *mut _,
            );
        }
        let msg = std::str::from_utf8(&log[..len as usize]).unwrap();
        panic!("Shader program error:\n{msg}");
    }
}
