use super::events::set_rdp_view_error;
use super::lifecycle::clear_rdp_reconnect_after_frame;
use crate::features::ZzClawTermApp;
use crate::features::remote_desktop::state::RemoteDesktopSessionState;
use gpui::{Bounds, DevicePixels, Point, Size, Window, point, size};
use zzclawterm_remote_desktop::{
    DirtyRect, Framebuffer, FramebufferLimits, RdpErrorKind, RdpFrameEvent, RemoteCursorEvent,
};

impl ZzClawTermApp {
    pub(super) fn stop_remote_desktop_uploads(
        &mut self,
        session_id: &str,
        window: &mut Window,
        kind: RdpErrorKind,
        message: String,
    ) {
        if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
            session.uploads_stopped = true;
            session.reconnect_at = None;
            session.framebuffer = None;
            session.cursor_shape = None;
            release_textures(session, window);
            set_rdp_view_error(session, kind, message);
        }
        // Capacity rejection keeps ownership in the manager. Maintenance retries
        // submission while uploads remain stopped.
        let _ = self.remote_desktop.manager.close_detached(session_id);
        let _ = self.remote_desktop.vnc_manager.close_detached(session_id);
    }

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
        if session.uploads_stopped {
            return;
        }
        release_textures(session, window);
        // Drop the previous allocation before acquiring its replacement quota.
        session.framebuffer = None;
        session.cursor_shape = None;
        session.cursor_position = Default::default();
        session.cursor_visible = true;
        match Framebuffer::new(epoch, width, height, limits) {
            Ok(framebuffer) => {
                session.framebuffer = Some(framebuffer);
                session.texture_dirty = true;
            }
            Err(error) => {
                let kind = if matches!(
                    error,
                    zzclawterm_remote_desktop::FramebufferError::AllocationFailed { .. }
                ) {
                    RdpErrorKind::ResourceExhausted
                } else {
                    RdpErrorKind::Protocol
                };
                self.stop_remote_desktop_uploads(
                    session_id,
                    window,
                    kind,
                    format!("remote desktop reset failed: {error}"),
                );
                return;
            }
        }
        if visible {
            self.prepare_remote_desktop_textures(session_id, window);
        }
    }

    pub(super) fn apply_rdp_frame_batch(
        &mut self,
        session_id: &str,
        frames: Vec<RdpFrameEvent>,
        window: &mut Window,
    ) {
        let visible = self.session.active_id() == Some(session_id);
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return;
        };
        if session.uploads_stopped {
            return;
        }
        let mut dirty = Vec::new();
        for frame in frames {
            let Some(framebuffer) = session.framebuffer.as_mut() else {
                continue;
            };
            match framebuffer.apply(&frame) {
                Ok(Some(rect)) => dirty.push(rect),
                Ok(None) | Err(zzclawterm_remote_desktop::FramebufferError::StaleEpoch { .. }) => {}
                Err(error) => {
                    self.stop_remote_desktop_uploads(
                        session_id,
                        window,
                        RdpErrorKind::Protocol,
                        format!("invalid remote desktop frame: {error}"),
                    );
                    return;
                }
            }
        }
        if !dirty.is_empty() {
            clear_rdp_reconnect_after_frame(session);
        }
        if !visible {
            session.texture_dirty |= !dirty.is_empty();
            return;
        }
        if session.texture_dirty || session.texture.is_none() {
            self.prepare_remote_desktop_textures(session_id, window);
            return;
        }
        let (Some(framebuffer), Some(texture)) = (session.framebuffer.as_ref(), session.texture)
        else {
            return;
        };
        let area = u64::from(framebuffer.width()) * u64::from(framebuffer.height());
        let dirty_area = dirty
            .iter()
            .map(|rect| u64::from(rect.width) * u64::from(rect.height))
            .sum::<u64>();
        let result = if dirty.len() > 64 || dirty_area.saturating_mul(100) >= area * 60 {
            window.update_dynamic_texture(
                texture,
                Bounds::new(
                    Point::new(DevicePixels(0), DevicePixels(0)),
                    Size::new(
                        DevicePixels(framebuffer.width() as i32),
                        DevicePixels(framebuffer.height() as i32),
                    ),
                ),
                framebuffer.pixels(),
                framebuffer.width() * 4,
            )
        } else {
            zzclawterm_remote_desktop::merge_dirty_rects(dirty)
                .into_iter()
                .try_for_each(|rect| upload_rdp_rect(window, texture, framebuffer, rect))
        };
        if let Err(error) = result {
            self.stop_remote_desktop_uploads(
                session_id,
                window,
                RdpErrorKind::ResourceExhausted,
                format!("remote desktop texture update failed: {error}"),
            );
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
        if session.uploads_stopped {
            return;
        }
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
                    session.cursor_shape = Some(shape);
                }
                RemoteCursorEvent::Position(position) => session.cursor_position = position,
                RemoteCursorEvent::Visibility(visibility) => {
                    session.cursor_visible = visibility.visible
                }
            }
        }
        if visible {
            self.prepare_remote_desktop_textures(session_id, window);
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
        if session.uploads_stopped {
            return;
        }
        if let Err(error) = prepare_textures(session, window) {
            self.stop_remote_desktop_uploads(
                session_id,
                window,
                RdpErrorKind::ResourceExhausted,
                format!("remote desktop texture upload failed: {error}"),
            );
        }
    }
}

fn release_textures(session: &mut RemoteDesktopSessionState, window: &mut Window) {
    if let Some(texture) = session.texture.take() {
        window.remove_dynamic_texture(texture);
    }
    if let Some(texture) = session.cursor_texture.take() {
        window.remove_dynamic_texture(texture);
    }
}

fn prepare_textures(
    session: &mut RemoteDesktopSessionState,
    window: &mut Window,
) -> anyhow::Result<()> {
    if let Some(framebuffer) = session.framebuffer.as_ref() {
        let dimensions = size(
            DevicePixels(framebuffer.width() as i32),
            DevicePixels(framebuffer.height() as i32),
        );
        if let Some(texture) = session.texture {
            if session.texture_dirty {
                window.update_dynamic_texture(
                    texture,
                    Bounds::new(Point::new(DevicePixels(0), DevicePixels(0)), dimensions),
                    framebuffer.pixels(),
                    framebuffer.width() * 4,
                )?;
            }
        } else {
            session.texture = Some(window.create_dynamic_texture(
                dimensions,
                framebuffer.pixels(),
                framebuffer.width() * 4,
            )?);
        }
        session.texture_dirty = false;
    }
    if session.cursor_texture.is_none()
        && let Some(shape) = session.cursor_shape.as_ref()
        && shape.width > 0
        && shape.height > 0
    {
        session.cursor_texture = Some(window.create_dynamic_texture(
            size(
                DevicePixels(shape.width as i32),
                DevicePixels(shape.height as i32),
            ),
            &shape.pixels,
            shape.width * 4,
        )?);
    }
    Ok(())
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
