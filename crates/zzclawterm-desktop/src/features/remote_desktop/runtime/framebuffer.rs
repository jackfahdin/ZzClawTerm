use super::events::set_rdp_view_error;
use super::lifecycle::clear_rdp_reconnect_after_frame;
use crate::features::ZzClawTermApp;
use gpui::Bounds;
use gpui::DevicePixels;
use gpui::Point;
use gpui::Size;
use gpui::Window;
use gpui::point;
use gpui::size;
use zzclawterm_remote_desktop::DirtyRect;
use zzclawterm_remote_desktop::Framebuffer;
use zzclawterm_remote_desktop::FramebufferLimits;
use zzclawterm_remote_desktop::RdpErrorKind;
use zzclawterm_remote_desktop::RdpFrameEvent;
use zzclawterm_remote_desktop::RemoteCursorEvent;

impl ZzClawTermApp {
    pub(super) fn reset_rdp_framebuffer(
        &mut self,
        session_id: &str,
        epoch: u64,
        width: u32,
        height: u32,
        limits: FramebufferLimits,
        window: &mut Window,
    ) {
        let visible = self.session.active_id() == Some(session_id);
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return;
        };
        if let Some(texture) = session.texture.take() {
            window.remove_dynamic_texture(texture);
        }
        if let Some(texture) = session.cursor_texture.take() {
            window.remove_dynamic_texture(texture);
        }
        match Framebuffer::new(epoch, width, height, limits) {
            Ok(framebuffer) => {
                if !visible {
                    session.framebuffer = Some(framebuffer);
                    session.texture_dirty = true;
                    session.cursor_shape = None;
                    session.cursor_position = Default::default();
                    session.cursor_visible = true;
                    return;
                }
                let texture_size = size(DevicePixels(width as i32), DevicePixels(height as i32));
                match window.create_dynamic_texture(texture_size, framebuffer.pixels(), width * 4) {
                    Ok(texture) => {
                        session.framebuffer = Some(framebuffer);
                        session.texture = Some(texture);
                        session.texture_dirty = false;
                        session.cursor_shape = None;
                        session.cursor_position = Default::default();
                        session.cursor_visible = true;
                    }
                    Err(error) => set_rdp_view_error(
                        session,
                        RdpErrorKind::Protocol,
                        format!("failed to create RDP texture: {error}"),
                    ),
                }
            }
            Err(error) => set_rdp_view_error(
                session,
                RdpErrorKind::Protocol,
                format!("invalid RDP desktop reset: {error}"),
            ),
        }
    }

    pub(super) fn apply_rdp_frame_batch(
        &mut self,
        session_id: &str,
        frames: Vec<RdpFrameEvent>,
        window: &mut Window,
    ) {
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return;
        };
        let mut dirty_rects = Vec::new();
        for frame in frames {
            let Some(framebuffer) = session.framebuffer.as_mut() else {
                continue;
            };
            match framebuffer.apply(&frame) {
                Ok(Some(rect)) => dirty_rects.push(rect),
                Ok(None) => {}
                Err(zzclawterm_remote_desktop::FramebufferError::StaleEpoch { .. }) => {}
                Err(error) => {
                    set_rdp_view_error(
                        session,
                        RdpErrorKind::Protocol,
                        format!("invalid RDP frame: {error}"),
                    );
                    return;
                }
            }
        }
        if !dirty_rects.is_empty() {
            clear_rdp_reconnect_after_frame(session);
        }
        if self.session.active_id() != Some(session_id) {
            session.texture_dirty |= !dirty_rects.is_empty();
            return;
        }
        if session.texture_dirty {
            self.prepare_remote_desktop_textures(session_id, window);
            return;
        }
        let (Some(framebuffer), Some(texture)) = (session.framebuffer.as_ref(), session.texture)
        else {
            return;
        };
        let framebuffer_area = u64::from(framebuffer.width()) * u64::from(framebuffer.height());
        let dirty_area = dirty_rects
            .iter()
            .map(|rect| u64::from(rect.width) * u64::from(rect.height))
            .sum::<u64>();
        if dirty_rects.len() > 64 || dirty_area.saturating_mul(100) >= framebuffer_area * 60 {
            let bounds = Bounds::new(
                Point::new(DevicePixels(0), DevicePixels(0)),
                Size::new(
                    DevicePixels(framebuffer.width() as i32),
                    DevicePixels(framebuffer.height() as i32),
                ),
            );
            let _ = window.update_dynamic_texture(
                texture,
                bounds,
                framebuffer.pixels(),
                framebuffer.width() * 4,
            );
            return;
        }
        for rect in zzclawterm_remote_desktop::merge_dirty_rects(dirty_rects) {
            let _ = upload_rdp_rect(window, texture, framebuffer, rect);
        }
    }

    pub(super) fn apply_remote_cursor_batch(
        &mut self,
        session_id: &str,
        cursors: Vec<RemoteCursorEvent>,
        window: &mut Window,
    ) {
        let visible = self.session.active_id() == Some(session_id);
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return;
        };
        for cursor in cursors {
            match cursor {
                RemoteCursorEvent::Shape(shape) => {
                    if session
                        .cursor_shape
                        .as_ref()
                        .is_some_and(|current| current.shape_id == shape.shape_id)
                        && session.cursor_texture.is_some()
                    {
                        continue;
                    }
                    if let Some(texture) = session.cursor_texture.take() {
                        window.remove_dynamic_texture(texture);
                    }
                    if visible
                        && shape.width > 0
                        && shape.height > 0
                        && let Ok(texture) = window.create_dynamic_texture(
                            size(
                                DevicePixels(shape.width as i32),
                                DevicePixels(shape.height as i32),
                            ),
                            &shape.pixels,
                            shape.width * 4,
                        )
                    {
                        session.cursor_texture = Some(texture);
                    }
                    session.cursor_shape = Some(shape);
                }
                RemoteCursorEvent::Position(position) => {
                    session.cursor_position = position;
                }
                RemoteCursorEvent::Visibility(visibility) => {
                    session.cursor_visible = visibility.visible;
                }
            }
        }
    }

    pub(in crate::features::remote_desktop) fn prepare_remote_desktop_textures(
        &mut self,
        session_id: &str,
        window: &mut Window,
    ) {
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return;
        };
        if let Some(framebuffer) = session.framebuffer.as_ref() {
            let dimensions = size(
                DevicePixels(framebuffer.width() as i32),
                DevicePixels(framebuffer.height() as i32),
            );
            let result = if let Some(texture) = session.texture {
                if session.texture_dirty {
                    window.update_dynamic_texture(
                        texture,
                        Bounds::new(Point::new(DevicePixels(0), DevicePixels(0)), dimensions),
                        framebuffer.pixels(),
                        framebuffer.width() * 4,
                    )
                } else {
                    Ok(())
                }
            } else {
                window
                    .create_dynamic_texture(
                        dimensions,
                        framebuffer.pixels(),
                        framebuffer.width() * 4,
                    )
                    .map(|texture| {
                        session.texture = Some(texture);
                    })
            };
            match result {
                Ok(()) => session.texture_dirty = false,
                Err(error) => set_rdp_view_error(
                    session,
                    RdpErrorKind::Protocol,
                    format!("failed to upload remote desktop texture: {error}"),
                ),
            }
        }
        if session.cursor_texture.is_none()
            && let Some(shape) = session.cursor_shape.as_ref()
            && shape.width > 0
            && shape.height > 0
        {
            session.cursor_texture = window
                .create_dynamic_texture(
                    size(
                        DevicePixels(shape.width as i32),
                        DevicePixels(shape.height as i32),
                    ),
                    &shape.pixels,
                    shape.width * 4,
                )
                .ok();
        }
    }
}

pub(super) fn upload_rdp_rect(
    window: &mut Window,
    texture: gpui::DynamicTexture,
    framebuffer: &Framebuffer,
    rect: DirtyRect,
) -> anyhow::Result<()> {
    let stride = framebuffer.width() * 4;
    let start = (u64::from(rect.y) * u64::from(stride) + u64::from(rect.x) * 4) as usize;
    let row_bytes = rect.width * 4;
    let len = (u64::from(rect.height - 1) * u64::from(stride) + u64::from(row_bytes)) as usize;
    let pixels = &framebuffer.pixels()[start..start + len];
    window.update_dynamic_texture(
        texture,
        Bounds::new(
            point(DevicePixels(rect.x as i32), DevicePixels(rect.y as i32)),
            size(
                DevicePixels(rect.width as i32),
                DevicePixels(rect.height as i32),
            ),
        ),
        pixels,
        stride,
    )
}
