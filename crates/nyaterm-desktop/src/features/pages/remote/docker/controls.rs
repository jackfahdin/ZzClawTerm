use gpui::{
    App, ClickEvent, Context, FontWeight, IntoElement, MouseButton, Window, div, prelude::*, px,
    rgb,
};
use nyaterm_transport::RemoteDockerOverview;
use nyaterm_ui::{NyaTabItem, NyaTabs};

use super::super::panels::RemoteMonitorPanel;
use crate::features::shell::gpui_code_font_family;
use crate::models::DockerTab;
use crate::theme::ThemePalette;

use super::DockerRenderContext;

pub(in crate::features::pages::remote) struct DockerTabBarLabels {
    pub tabs: [String; 5],
    pub more: String,
}

pub(in crate::features::pages::remote) fn docker_overview_strip(
    palette: ThemePalette,
    overview: &RemoteDockerOverview,
    labels: [String; 3],
) -> impl IntoElement {
    let [running_label, stopped_label, images_label] = labels;
    let running = overview
        .containers
        .iter()
        .filter(|container| container.state.eq_ignore_ascii_case("running"))
        .count();
    let stopped = overview.containers.len().saturating_sub(running);

    div()
        .h(px(32.))
        .flex_none()
        .mx_2()
        .mt_2()
        .mb_2()
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.section_header))
        .px_2()
        .flex()
        .items_center()
        .justify_between()
        .gap_1()
        .child(docker_overview_stat(
            palette,
            running_label,
            running,
            Some(0x86efac),
        ))
        .child(docker_overview_stat(
            palette,
            stopped_label,
            stopped,
            Some(0xcbd5e1),
        ))
        .child(docker_overview_stat(
            palette,
            images_label,
            overview.images.len(),
            None,
        ))
}

fn docker_overview_stat(
    palette: ThemePalette,
    label: String,
    value: usize,
    accent: Option<u32>,
) -> impl IntoElement {
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap_1()
        .text_size(px(10.))
        .text_color(accent.map(rgb).unwrap_or_else(|| rgb(palette.text_muted)))
        .child(
            div()
                .flex_none()
                .text_color(rgb(palette.text_muted))
                .child(label),
        )
        .child(
            div()
                .flex_none()
                .font_family(gpui_code_font_family())
                .text_size(px(11.))
                .font_weight(FontWeight(600.))
                .child(value.to_string()),
        )
}

pub(in crate::features::pages::remote) fn docker_tab_bar(
    context: DockerRenderContext,
    active_tab: DockerTab,
    overview: &RemoteDockerOverview,
    labels: DockerTabBarLabels,
    panel_width: f32,
    menu_open: bool,
    cx: &mut Context<RemoteMonitorPanel>,
) -> impl IntoElement {
    let DockerRenderContext {
        palette, menu_bg, ..
    } = context;
    let DockerTabBarLabels {
        tabs: labels,
        more: more_label,
    } = labels;
    let [
        containers_label,
        images_label,
        volumes_label,
        networks_label,
        compose_label,
    ] = labels;
    let mut tabs = vec![
        (
            DockerTab::Containers,
            format!("{} {}", containers_label, overview.containers.len()),
        ),
        (
            DockerTab::Images,
            format!("{} {}", images_label, overview.images.len()),
        ),
        (
            DockerTab::Volumes,
            format!("{} {}", volumes_label, overview.volumes.len()),
        ),
        (
            DockerTab::Networks,
            format!("{} {}", networks_label, overview.networks.len()),
        ),
    ];
    if overview.compose_available {
        tabs.push((
            DockerTab::Compose,
            format!("{} {}", compose_label, overview.compose_projects.len()),
        ));
    }
    // Tauri switches overflowed tabs into a More menu. These thresholds keep
    // the GPUI tab strip stable while preserving access to every tab.
    let visible_count = if panel_width > 0. && panel_width < 300. {
        1
    } else if panel_width > 0. && panel_width < 390. {
        2
    } else if panel_width > 0. && panel_width < 500. {
        3
    } else if panel_width > 0. && panel_width < 620. {
        4
    } else {
        tabs.len()
    };
    let visible_count = visible_count.min(tabs.len());
    let visible_tabs = &tabs[..visible_count];
    let hidden_tabs = &tabs[visible_count..];
    let more_active = hidden_tabs.iter().any(|(tab, _)| *tab == active_tab);
    let visible_tab_values = visible_tabs.iter().map(|(tab, _)| *tab).collect::<Vec<_>>();
    let mut bar = div()
        .id("docker-tab-bar")
        .relative()
        .h(px(32.))
        .flex_none()
        .px_2()
        .border_b_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.section_header))
        .flex()
        .items_center()
        .gap_1();
    bar = bar.child(
        div().min_w_0().flex_1().child(
            NyaTabs::new("docker-tabs")
                .items(
                    visible_tabs
                        .iter()
                        .map(|(_, label)| NyaTabItem::new((*label).clone())),
                )
                .selected_index_if_visible(
                    visible_tabs.iter().position(|(tab, _)| *tab == active_tab),
                )
                .on_select(cx.listener(move |panel, index: &usize, _, cx| {
                    panel.with_app(cx, |this, cx| {
                        let Some(tab) = visible_tab_values.get(*index).copied() else {
                            return;
                        };
                        this.set_docker_tab(tab, cx);
                    });
                })),
        ),
    );
    if !hidden_tabs.is_empty() {
        bar = bar.child(
            div()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(docker_tab_button(
                    palette,
                    "docker-tab-more",
                    more_label,
                    menu_open || more_active,
                    cx.listener(|panel, _, _, cx| {
                        panel.with_app(cx, |this, cx| {
                            this.toggle_docker_tab_menu(cx);
                        });
                    }),
                )),
        );
        if menu_open {
            let mut menu = div()
                .id("docker-tab-more-menu")
                .absolute()
                .top(px(30.))
                .right(px(4.))
                .w(px(160.))
                .rounded_md()
                .border_1()
                .border_color(rgb(palette.border))
                .bg(menu_bg)
                .shadow_lg()
                .py_1()
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
            for (index, (tab, label)) in hidden_tabs.iter().enumerate() {
                let tab = *tab;
                let compose_disabled = tab == DockerTab::Compose && !overview.compose_available;
                menu = menu.child(docker_tab_menu_item(
                    palette,
                    format!("docker-tab-more-{index}"),
                    label.clone(),
                    active_tab == tab,
                    compose_disabled,
                    cx.listener(move |panel, _, _, cx| {
                        panel.with_app(cx, |this, cx| {
                            this.set_docker_tab(tab, cx);
                        });
                    }),
                ));
            }
            bar = bar.child(menu);
        }
    }
    bar
}

fn docker_tab_button(
    palette: ThemePalette,
    id: impl Into<String>,
    label: String,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(gpui::SharedString::from(id.into()))
        .h(px(24.))
        .px_2()
        .flex()
        .items_center()
        .rounded_sm()
        .bg(if active {
            rgb(palette.surface_elevated)
        } else {
            rgb(palette.bg)
        })
        .text_color(if active {
            rgb(palette.text)
        } else {
            rgb(palette.text_muted)
        })
        .text_size(px(11.))
        .font_weight(if active {
            FontWeight(600.)
        } else {
            FontWeight(500.)
        })
        .cursor_pointer()
        .hover(|this| this.bg(rgb(palette.hover)).text_color(rgb(palette.text)))
        .child(label)
        .on_click(on_click)
}

fn docker_tab_menu_item(
    palette: ThemePalette,
    id: String,
    label: String,
    active: bool,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(gpui::SharedString::from(id))
        .h(px(28.))
        .px_3()
        .flex()
        .items_center()
        .text_size(px(11.))
        .text_color(if disabled {
            rgb(palette.text_dimmed)
        } else {
            rgb(palette.text)
        })
        .bg(if active {
            rgb(palette.hover)
        } else {
            rgb(palette.surface_elevated)
        })
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(|this| this.bg(rgb(palette.hover)))
        })
        .child(label)
        .when(!disabled, |this| this.on_click(on_click))
}
